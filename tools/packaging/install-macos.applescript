on run
    try
        display dialog "Install student-mcp in Applications and open it? Quit any running copy before continuing." with title "Install student-mcp" buttons {"Cancel", "Install and open"} default button "Install and open"
        set sourceApp to POSIX path of (path to resource "student-mcp.app")
        set installerScript to POSIX path of (path to resource "install-macos.sh")
        set installCommand to "/bin/bash " & quoted form of installerScript & " " & quoted form of sourceApp
        -- Request administrator approval only if Applications is not writable.
        try
            do shell script "/bin/test -w /Applications"
            do shell script "/bin/test ! -e /Applications/student-mcp.app || /bin/test -w /Applications/student-mcp.app"
            set needsAdmin to false
        on error
            set needsAdmin to true
        end try
        if needsAdmin then
            do shell script installCommand with administrator privileges
        else
            do shell script installCommand
        end if
        -- Run as the logged-in user, never from the privileged copy command.
        do shell script "/usr/bin/open /Applications/student-mcp.app"
    on error messageText number errorNumber
        if errorNumber is not -128 then
            display alert "student-mcp could not be installed" message messageText as critical
        end if
    end try
end run
