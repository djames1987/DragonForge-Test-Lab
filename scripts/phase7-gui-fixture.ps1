param()

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$form = New-Object System.Windows.Forms.Form
$form.Text = "DragonForge-GUI-Fixture"
$form.Width = 520
$form.Height = 260
$form.StartPosition = "CenterScreen"

$label = New-Object System.Windows.Forms.Label
$label.Text = "Input"
$label.Left = 24
$label.Top = 25
$label.Width = 80
$form.Controls.Add($label)

$input = New-Object System.Windows.Forms.TextBox
$input.Name = "inputBox"
$input.AccessibleName = "inputBox"
$input.AccessibleDescription = "inputBox"
$input.Left = 110
$input.Top = 20
$input.Width = 350
$form.Controls.Add($input)

$outputLabel = New-Object System.Windows.Forms.Label
$outputLabel.Text = "Output"
$outputLabel.Left = 24
$outputLabel.Top = 72
$outputLabel.Width = 80
$form.Controls.Add($outputLabel)

$output = New-Object System.Windows.Forms.TextBox
$output.Name = "outputBox"
$output.AccessibleName = "outputBox"
$output.AccessibleDescription = "outputBox"
$output.Left = 110
$output.Top = 67
$output.Width = 350
$output.ReadOnly = $true
$form.Controls.Add($output)

$apply = New-Object System.Windows.Forms.Button
$apply.Name = "applyButton"
$apply.AccessibleName = "applyButton"
$apply.AccessibleDescription = "applyButton"
$apply.Text = "Apply"
$apply.Left = 110
$apply.Top = 120
$apply.Width = 110
$apply.Add_Click({
    $output.Text = $input.Text
})
$form.Controls.Add($apply)

$crash = New-Object System.Windows.Forms.Button
$crash.Name = "crashButton"
$crash.AccessibleName = "crashButton"
$crash.AccessibleDescription = "crashButton"
$crash.Text = "Crash Fixture"
$crash.Left = 240
$crash.Top = 120
$crash.Width = 130
$crash.Add_Click({
    [Environment]::Exit(23)
})
$form.Controls.Add($crash)

[void]$form.ShowDialog()
