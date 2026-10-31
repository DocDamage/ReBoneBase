$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $projectRoot
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    $metadata = cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Could not read package metadata' }
    $package = $metadata.packages | Where-Object { $_.name -eq 'rebonebase' }
    if (-not $package.version) { throw 'ReBoneBase package version is missing' }
    $executableName = "ReBoneBase-$($package.version).exe"
    $outputPath = Join-Path $projectRoot "dist\ReBoneBase-$($package.version)"
    New-Item -ItemType Directory -Path $outputPath -Force | Out-Null
    $executablePath = Join-Path $outputPath $executableName
    Copy-Item -LiteralPath (Join-Path $metadata.target_directory 'release\rebonebase.exe') -Destination $executablePath
    Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md') -Destination $outputPath
    Copy-Item -LiteralPath (Join-Path $projectRoot 'BONEBASE_LICENSE.txt') -Destination $outputPath
    $docsPath = Join-Path $outputPath 'docs'
    New-Item -ItemType Directory -Path $docsPath -Force | Out-Null
    foreach ($guide in @('STUDIO_GUIDE.md', 'FEATURE_IDEAS.md', 'PRODUCTION_GUIDE.md', 'PRODUCTION_FEATURES.md', 'CHARACTERS_GUIDE.md', 'ANIMATION_IMPORT_GUIDE.md', 'RELIABILITY.md')) {
        Copy-Item -LiteralPath (Join-Path $projectRoot "docs\$guide") -Destination $docsPath
    }
    $integrationPath = Join-Path $outputPath 'integrations\unreal'
    New-Item -ItemType Directory -Path $integrationPath -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot 'integrations\unreal\export_animations.py') -Destination $integrationPath
    Copy-Item -LiteralPath (Join-Path $projectRoot 'docs\UFBX_LICENSE.txt') -Destination $docsPath
    foreach ($file in @('CharactersDemo.rebone','AnimationTemplates.rebone')) {
        $samplePath = Join-Path $projectRoot ('examples\output\characters\' + $file)
        if (Test-Path -LiteralPath $samplePath) { Copy-Item -LiteralPath $samplePath -Destination $outputPath }
    }
    Copy-Item -LiteralPath (Join-Path $projectRoot 'assets\characters\Mannequin.obj') -Destination $outputPath
    Copy-Item -LiteralPath (Join-Path $projectRoot 'assets\characters\Mannequin.mtl') -Destination $outputPath
    foreach ($file in @('MotionImportDemo.rebone','SharedSway.bvh')) {
        $motionSample = Join-Path $projectRoot ('examples\output\motion\' + $file)
        if (Test-Path -LiteralPath $motionSample) { Copy-Item -LiteralPath $motionSample -Destination $outputPath }
    }
    $productionDemo = Join-Path $projectRoot 'examples\output\production\ProductionDemo.rebone'
    if (Test-Path -LiteralPath $productionDemo) { Copy-Item -LiteralPath $productionDemo -Destination $outputPath }
    $demoPath = Join-Path $projectRoot 'examples\output\Wave.rebone'
    if (Test-Path -LiteralPath $demoPath) { Copy-Item -LiteralPath $demoPath -Destination $outputPath }
    $digest = (Get-FileHash -LiteralPath $executablePath -Algorithm SHA256).Hash.ToLowerInvariant()
    "$digest  $executableName" | Set-Content -LiteralPath "$executablePath.sha256" -Encoding ascii
    Get-Item -LiteralPath $executablePath | Select-Object FullName,Length
} finally { Pop-Location }
