//! Desktop entry point; the main binary keeps its no-argument stdio contract.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::process::Command;

fn launch() -> std::io::Result<()> {
    let executable = std::env::current_exe()?;
    let binary = executable.with_file_name(if cfg!(windows) {
        "student-mcp.exe"
    } else {
        "student-mcp"
    });
    let mut command = Command::new(binary);
    command.arg("up");

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Do not flash a console when launched from the Start menu or installer.
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Keep the app-bundle process alive for Dock/menu-bar lifecycle handling.
        Err(command.exec())
    }
    #[cfg(not(unix))]
    command.spawn().map(|_| ())
}

fn main() {
    if let Err(error) = launch() {
        let message = format!("Could not open student-mcp: {error}");
        #[cfg(windows)]
        {
            #[link(name = "user32")]
            extern "system" {
                fn MessageBoxW(
                    window: *mut std::ffi::c_void,
                    text: *const u16,
                    title: *const u16,
                    flags: u32,
                ) -> i32;
            }
            let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
            let title: Vec<u16> = "student-mcp".encode_utf16().chain(Some(0)).collect();
            // Both strings remain NUL-terminated and alive throughout the call.
            unsafe {
                MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x10);
            }
        }
        #[cfg(not(windows))]
        eprintln!("{message}");
        std::process::exit(1);
    }
}
