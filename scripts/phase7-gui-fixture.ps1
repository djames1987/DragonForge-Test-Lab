param()

$ErrorActionPreference = "Stop"

Add-Type -AssemblyName PresentationFramework
Add-Type -AssemblyName PresentationCore
Add-Type -AssemblyName WindowsBase

[xml]$xaml = @"
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Title="DragonForge-GUI-Fixture"
        Width="520"
        Height="260"
        WindowStartupLocation="CenterScreen"
        ResizeMode="NoResize">
  <Grid Margin="20">
    <Grid.RowDefinitions>
      <RowDefinition Height="Auto"/>
      <RowDefinition Height="Auto"/>
      <RowDefinition Height="Auto"/>
      <RowDefinition Height="*"/>
    </Grid.RowDefinitions>
    <Grid.ColumnDefinitions>
      <ColumnDefinition Width="80"/>
      <ColumnDefinition Width="*"/>
    </Grid.ColumnDefinitions>

    <TextBlock Grid.Row="0" Grid.Column="0" Margin="0,4,10,12" Text="Input"/>
    <TextBox x:Name="inputBox"
             AutomationProperties.AutomationId="inputBox"
             Grid.Row="0"
             Grid.Column="1"
             Height="28"
             Margin="0,0,0,12"/>

    <TextBlock Grid.Row="1" Grid.Column="0" Margin="0,4,10,12" Text="Output"/>
    <TextBox x:Name="outputBox"
             AutomationProperties.AutomationId="outputBox"
             Grid.Row="1"
             Grid.Column="1"
             Height="28"
             Margin="0,0,0,12"
             IsReadOnly="True"/>

    <StackPanel Grid.Row="2" Grid.Column="1" Orientation="Horizontal">
      <Button x:Name="applyButton"
              AutomationProperties.AutomationId="applyButton"
              Width="110"
              Height="32"
              Margin="0,0,14,0"
              Content="Apply"/>
      <Button x:Name="crashButton"
              AutomationProperties.AutomationId="crashButton"
              Width="130"
              Height="32"
              Content="Crash Fixture"/>
    </StackPanel>
  </Grid>
</Window>
"@

$reader = New-Object System.Xml.XmlNodeReader $xaml
$window = [Windows.Markup.XamlReader]::Load($reader)

$input = $window.FindName("inputBox")
$output = $window.FindName("outputBox")
$apply = $window.FindName("applyButton")
$crash = $window.FindName("crashButton")

$apply.Add_Click({
    $output.Text = $input.Text
})

$crash.Add_Click({
    [Environment]::Exit(23)
})

[void]$window.ShowDialog()
