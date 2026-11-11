//! Optional Office rendering. Never run LibreOffice outside an OS sandbox.
use std::sync::Arc;

pub(super) fn supported(filename: &str) -> bool {
    matches!(
        extension(filename).as_str(),
        "docx" | "docm" | "pptx" | "pptm" | "odt" | "odp" | "doc" | "ppt"
    )
}

fn extension(filename: &str) -> String {
    filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub(super) fn convert(filename: &str, bytes: &[u8]) -> Result<Arc<Vec<u8>>, String> {
    if !supported(filename) || bytes.len() > 12 * 1024 * 1024 {
        return Err("Office conversion input is unsupported or too large.".into());
    }
    if !matches!(extension(filename).as_str(), "doc" | "ppt") {
        crate::document::validate_zip(bytes)?;
    }
    convert_isolated(filename, bytes)
}

#[cfg(not(target_os = "linux"))]
fn convert_isolated(_: &str, _: &[u8]) -> Result<Arc<Vec<u8>>, String> {
    Err("Isolated Office-to-PDF conversion is unavailable on this platform. Text fallback may omit charts, equations and layout.".into())
}

#[cfg(target_os = "linux")]
fn convert_isolated(filename: &str, bytes: &[u8]) -> Result<Arc<Vec<u8>>, String> {
    use std::{io::Read, path::Path};
    let unavailable = "Isolated Office-to-PDF conversion unavailable or failed (requires LibreOffice, Bubblewrap and prlimit). Text fallback may omit charts, equations and layout.";
    // Only system-installed executables: no helper scripts or user configuration.
    let executable = ["/usr/bin/libreoffice", "/usr/bin/soffice"]
        .into_iter()
        .find(|path| Path::new(path).is_file())
        .ok_or(unavailable)?;
    if !Path::new("/usr/bin/bwrap").is_file() || !Path::new("/usr/bin/prlimit").is_file() {
        return Err(unavailable.into());
    }
    let dir = crate::pdf::Workspace::new().ok_or(unavailable)?;
    std::fs::create_dir_all(dir.join("profile/user")).map_err(|_| unavailable)?;
    std::fs::create_dir(dir.join("out")).map_err(|_| unavailable)?;
    let input = format!("input.{}", extension(filename));
    std::fs::write(dir.join(&input), bytes).map_err(|_| unavailable)?;
    // Fresh profile, no trusted locations/certificates, highest macro security.
    std::fs::write(dir.join("profile/user/registrymodifications.xcu"), PROFILE)
        .map_err(|_| unavailable)?;
    let mut command = sandbox_command(&dir);
    command
        .args([
            executable,
            "-env:UserInstallation=file:///work/profile",
            "--headless",
            "--nologo",
            "--nodefault",
            "--nofirststartwizard",
            "--norestore",
            "--convert-to",
            "pdf",
            "--outdir",
            "/work/out",
        ])
        .arg(format!("/work/{input}"));
    let output = crate::pdf::output(&mut command).map_err(|_| unavailable)?;
    if !output.status.success() {
        return Err(unavailable.into());
    }
    let mut pdf = Vec::new();
    std::fs::File::open(dir.join("out/input.pdf"))
        .map_err(|_| unavailable)?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut pdf)
        .map_err(|_| unavailable)?;
    if pdf.len() > 32 * 1024 * 1024 || !pdf.starts_with(b"%PDF") {
        return Err(unavailable.into());
    }
    Ok(Arc::new(pdf))
}

#[cfg(target_os = "linux")]
fn sandbox_command(dir: &std::path::Path) -> std::process::Command {
    let mut command = std::process::Command::new("/usr/bin/bwrap");
    command
        .args([
            "--unshare-all",
            "--unshare-user",
            "--disable-userns",
            "--die-with-parent",
            "--new-session",
            "--cap-drop",
            "ALL",
            "--clearenv",
            "--setenv",
            "HOME",
            "/work/profile",
            "--setenv",
            "TMPDIR",
            "/tmp",
            "--setenv",
            "PATH",
            "/usr/bin:/bin",
            "--setenv",
            "LANG",
            "C.UTF-8",
            "--setenv",
            "SAL_USE_VCLPLUGIN",
            "svp",
            "--ro-bind",
            "/usr",
            "/usr",
            "--ro-bind-try",
            "/lib",
            "/lib",
            "--ro-bind-try",
            "/lib64",
            "/lib64",
            "--ro-bind-try",
            "/bin",
            "/bin",
            "--dir",
            "/etc",
            "--ro-bind-try",
            "/etc/fonts",
            "/etc/fonts",
            "--ro-bind-try",
            "/etc/ld.so.cache",
            "/etc/ld.so.cache",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--size",
            "134217728",
            "--tmpfs",
            "/tmp",
            "--bind",
        ])
        .arg(dir)
        .arg("/work")
        .args([
            "--chdir",
            "/work",
            "--",
            "/usr/bin/prlimit",
            "--fsize=33554432",
            "--as=2147483648",
            "--cpu=25",
            "--",
        ]);
    command
}

#[cfg(target_os = "linux")]
const PROFILE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<oor:items xmlns:oor="http://openoffice.org/2001/registry">
<item oor:path="/org.openoffice.Office.Common/Security/Scripting">
<prop oor:name="MacroSecurityLevel" oor:op="fuse"><value>3</value></prop>
<prop oor:name="DisableMacrosExecution" oor:op="fuse"><value>true</value></prop>
</item></oor:items>"#;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spreadsheets_never_enter_the_converter_and_invalid_packages_fail() {
        assert!(!supported("table.xlsx"));
        assert!(!supported("table.ods"));
        assert!(supported("slides.PPTX"));
        assert!(convert("bad.docx", b"not a zip").is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires Bubblewrap and permitted unprivileged namespaces; synthetic data only"]
    fn office_sandbox_hides_home_environment_and_host_network() {
        let dir = crate::pdf::Workspace::new().unwrap();
        let host_network = std::fs::read_link("/proc/self/ns/net").unwrap();
        let output = crate::pdf::output(sandbox_command(&dir)
            .env("STUDENT_MCP_TEST_SECRET", "never-inherit-this")
            .args(["/bin/sh", "-c", "test ! -e /home && test ! -e /run/user && test -z \"$STUDENT_MCP_TEST_SECRET\" && readlink /proc/self/ns/net"]))
            .unwrap();
        assert!(output.status.success());
        assert_ne!(
            String::from_utf8_lossy(&output.stdout).trim(),
            host_network.to_str().unwrap()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires system LibreOffice, Bubblewrap, prlimit and Poppler; synthetic DOCX only"]
    fn office_conversion_renders_a_real_docx() {
        let bytes = crate::document::tests::archive(&[
            (
                "[Content_Types].xml",
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
            ),
            (
                "_rels/.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
            ),
            (
                "word/document.xml",
                r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Synthetic lecture for isolated conversion</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#,
            ),
        ]);
        let pdf = convert("lecture.docx", &bytes).unwrap();
        let text = super::super::pdf::pdftotext_cli_range(&pdf, 1, 1).unwrap();
        assert!(text.contains("Synthetic lecture"));
        assert_eq!(super::super::pdf::render_pdf_pages(&pdf, 1, 1).len(), 1);
    }
}
