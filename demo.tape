# VHS tape file for recording aghist demo GIF
# Install VHS: https://github.com/charmbracelet/vhs
# Run: vhs demo.tape

Output demo.gif

Set Shell bash
Set FontSize 14
Set Width 1200
Set Height 700
Set Padding 10
Set Theme "Catppuccin Mocha"

Type "aghist"
Enter
Sleep 2s

# Navigate sessions
Sleep 1s
Down
Sleep 500ms
Down
Sleep 500ms
Down
Sleep 500ms

# Open a session
Enter
Sleep 1.5s

# Scroll through messages
Down
Sleep 300ms
Down
Sleep 300ms
Down
Sleep 300ms
Down
Sleep 300ms
Down
Sleep 300ms

# Toggle tool calls
Type "t"
Sleep 1.5s

# Go back
Escape
Sleep 1s

# Open search
Type "/"
Sleep 500ms
Type "implement"
Sleep 1.5s
Escape
Sleep 1s

# Open filter panel
Type "f"
Sleep 1.5s
Escape
Sleep 1s

# Show help
Type "?"
Sleep 2s
Escape
Sleep 1s

# Quit
Type "q"
Sleep 500ms
