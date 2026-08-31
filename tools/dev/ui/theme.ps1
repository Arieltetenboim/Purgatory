# WinForms theme and control factories.

function New-DesaturatedColor {
    param(
        [int]$R,
        [int]$G,
        [int]$B,
        [double]$Amount = 0.20
    )

    $r0 = $R / 255.0
    $g0 = $G / 255.0
    $b0 = $B / 255.0
    $max = [Math]::Max($r0, [Math]::Max($g0, $b0))
    $min = [Math]::Min($r0, [Math]::Min($g0, $b0))
    $l = ($max + $min) / 2.0
    $d = $max - $min
    if ($d -lt 1e-6) {
        return [Drawing.Color]::FromArgb($R, $G, $B)
    }

    $s = $d / (1.0 - [Math]::Abs((2.0 * $l) - 1.0))
    $s = $s * (1.0 - $Amount)

    if ($max -eq $r0) {
        $h = (($g0 - $b0) / $d) % 6.0
        if ($h -lt 0) { $h += 6.0 }
    }
    elseif ($max -eq $g0) {
        $h = (($b0 - $r0) / $d) + 2.0
    }
    else {
        $h = (($r0 - $g0) / $d) + 4.0
    }

    $c = (1.0 - [Math]::Abs((2.0 * $l) - 1.0)) * $s
    $x = $c * (1.0 - [Math]::Abs(($h % 2.0) - 1.0))
    $m = $l - ($c / 2.0)
    $rp = 0.0
    $gp = 0.0
    $bp = 0.0
    if ($h -lt 1) { $rp = $c; $gp = $x }
    elseif ($h -lt 2) { $rp = $x; $gp = $c }
    elseif ($h -lt 3) { $gp = $c; $bp = $x }
    elseif ($h -lt 4) { $gp = $x; $bp = $c }
    elseif ($h -lt 5) { $rp = $x; $bp = $c }
    else { $rp = $c; $bp = $x }

    $to8 = {
        param($V)
        [int][Math]::Max(0, [Math]::Min(255, [Math]::Round($V * 255.0)))
    }
    return [Drawing.Color]::FromArgb((& $to8 ($rp + $m)), (& $to8 ($gp + $m)), (& $to8 ($bp + $m)))
}

function Initialize-DevTheme {
    $script:Bg = New-DesaturatedColor 12 10 9
    $script:Panel = New-DesaturatedColor 26 22 20
    $script:PanelHi = New-DesaturatedColor 36 30 26
    $script:Border = New-DesaturatedColor 74 50 36
    $script:Text = New-DesaturatedColor 242 232 220
    $script:Muted = New-DesaturatedColor 160 144 128
    $script:Accent = New-DesaturatedColor 232 120 48
    $script:Green = New-DesaturatedColor 95 212 138
    $script:Red = New-DesaturatedColor 232 90 76
    $script:Yellow = New-DesaturatedColor 232 184 74
    $script:LogColor = New-DesaturatedColor 196 176 156
    $script:LogServerColor = New-DesaturatedColor 95 212 138
    $script:LogClientColor = New-DesaturatedColor 88 186 220
    $script:LogProbeColor = New-DesaturatedColor 232 184 74
    $script:LogCargoColor = New-DesaturatedColor 160 144 128
    $script:Disabled = [Drawing.Color]::FromArgb(156, 156, 156)
    $script:BtnPress = [Drawing.Color]::FromArgb(48, 38, 30)
    $Bg = $script:Bg
    $Panel = $script:Panel
    $PanelHi = $script:PanelHi
    $Border = $script:Border
    $Text = $script:Text
    $Muted = $script:Muted
    $Accent = $script:Accent
    $Green = $script:Green
    $Red = $script:Red
    $Yellow = $script:Yellow
    $LogColor = $script:LogColor
    $Disabled = $script:Disabled
    $BtnPress = $script:BtnPress
}

function New-Font {
    param([float]$Size, [Drawing.FontStyle]$Style = [Drawing.FontStyle]::Regular)
    return New-Object Drawing.Font("Segoe UI", $Size, $Style)
}

function New-Label {
    param(
        $Parent,
        [string]$Text,
        [int]$X,
        [int]$Y,
        [int]$Width = 0,
        [Drawing.Color]$Color = $script:Text,
        [float]$Size = 9,
        [Drawing.FontStyle]$Style = [Drawing.FontStyle]::Regular
    )

    $label = New-Object Windows.Forms.Label
    $label.Text = $Text
    $label.UseMnemonic = $false
    $label.Location = New-Object Drawing.Point($X, $Y)
    $label.ForeColor = $Color
    $label.BackColor = [Drawing.Color]::Transparent
    $label.Font = New-Font $Size $Style
    if ($Width -gt 0) {
        $label.AutoSize = $false
        $label.Size = New-Object Drawing.Size($Width, [int]($Size + 12))
    }
    else {
        $label.AutoSize = $true
    }
    $Parent.Controls.Add($label)
    return $label
}

function New-Card {
    param($Parent, [int]$X, [int]$Y, [int]$Width, [int]$Height)

    $edge = New-Object Windows.Forms.Panel
    $edge.Location = New-Object Drawing.Point($X, $Y)
    $edge.Size = New-Object Drawing.Size($Width, $Height)
    $edge.BackColor = $script:Border
    $Parent.Controls.Add($edge)

    $inner = New-Object Windows.Forms.Panel
    $inner.Location = New-Object Drawing.Point(1, 1)
    $inner.Size = New-Object Drawing.Size(($Width - 2), ($Height - 2))
    $inner.BackColor = $script:Panel
    $edge.Controls.Add($inner)
    return $inner
}

function New-Button {
    param(
        $Parent,
        [string]$Text,
        [int]$X,
        [int]$Y,
        [int]$Width,
        [Drawing.Color]$Color,
        [scriptblock]$Click,
        [string]$Tip = "",
        [int]$Height = 40
    )

    $button = New-Object Windows.Forms.Button
    $button.Text = $Text
    $button.Location = New-Object Drawing.Point($X, $Y)
    $button.Size = New-Object Drawing.Size($Width, $Height)
    $button.FlatStyle = "Flat"
    $button.FlatAppearance.BorderSize = 1
    $button.FlatAppearance.BorderColor = $Color
    $button.FlatAppearance.MouseOverBackColor = $script:PanelHi
    $button.FlatAppearance.MouseDownBackColor = $script:BtnPress
    $button.BackColor = $script:Panel
    $button.ForeColor = $Color
    $button.Font = New-Font 9 ([Drawing.FontStyle]::Bold)
    $button.Cursor = [Windows.Forms.Cursors]::Hand
    $button.TabStop = $true
    if ($Tip -and $script:Tips) {
        $script:Tips.SetToolTip($button, $Tip)
    }
    $button.Add_Click($Click)
    $Parent.Controls.Add($button)
    return $button
}

function Set-ToggleButton {
    param(
        $Button,
        [bool]$IsEnabled,
        $ActiveColor
    )

    $Button.Tag = $IsEnabled
    $Button.TabStop = $IsEnabled
    $Button.BackColor = $script:Panel
    if ($IsEnabled) {
        $Button.ForeColor = $ActiveColor
        $Button.FlatAppearance.BorderColor = $ActiveColor
        $Button.FlatAppearance.MouseOverBackColor = $script:PanelHi
        $Button.FlatAppearance.MouseDownBackColor = $script:BtnPress
        $Button.Cursor = [Windows.Forms.Cursors]::Hand
    }
    else {
        $Button.ForeColor = $script:Disabled
        $Button.FlatAppearance.BorderColor = $script:Disabled
        $Button.FlatAppearance.MouseOverBackColor = $script:Panel
        $Button.FlatAppearance.MouseDownBackColor = $script:Panel
        $Button.Cursor = [Windows.Forms.Cursors]::Default
    }
}

function Set-ProfileButtons {
    if ($script:BuildProfile -eq "release") {
        $script:BtnDebug.BackColor = $script:Panel
        $script:BtnDebug.ForeColor = $script:Muted
        $script:BtnDebug.FlatAppearance.BorderColor = $script:Border
        $script:BtnRelease.BackColor = $script:PanelHi
        $script:BtnRelease.ForeColor = $script:Accent
        $script:BtnRelease.FlatAppearance.BorderColor = $script:Accent
    }
    else {
        $script:BtnDebug.BackColor = $script:PanelHi
        $script:BtnDebug.ForeColor = $script:Accent
        $script:BtnDebug.FlatAppearance.BorderColor = $script:Accent
        $script:BtnRelease.BackColor = $script:Panel
        $script:BtnRelease.ForeColor = $script:Muted
        $script:BtnRelease.FlatAppearance.BorderColor = $script:Border
    }
}

function Set-LabelIfChanged {
    param($Label, [string]$Value, $Color = $null)

    if ($null -eq $Label) { return }
    try {
        if ($Label.IsDisposed) { return }
        if ($Label.Text -ne $Value) { $Label.Text = $Value }
        if ($null -ne $Color -and $Label.ForeColor -ne $Color) { $Label.ForeColor = $Color }
    }
    catch { }
}

function Get-LogoImage {
    $path = Join-Path (Join-Path $script:Root "Graphic") "LOGO.png"
    if (-not (Test-Path -LiteralPath $path)) { return $null }
    try {
        $bytes = [IO.File]::ReadAllBytes($path)
        $ms = New-Object IO.MemoryStream(, $bytes)
        return [Drawing.Image]::FromStream($ms)
    }
    catch {
        return $null
    }
}
