$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $projectRoot
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    $outputPath = Join-Path $projectRoot 'dist'
    New-Item -ItemType Directory -Path $outputPath -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot 'target\release\rebonebase.exe') -Destination (Join-Path $outputPath 'ReBoneBase-motion.exe')
    Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md') -Destination $outputPath
    Copy-Item -LiteralPath (Join-Path $projectRoot 'BONEBASE_LICENSE.txt') -Destination $outputPath
    Copy-Item -LiteralPath (Join-Path $projectRoot 'docs\STUDIO_GUIDE.md') -Destination $outputPath
    Copy-Item -LiteralPath (Join-Path $projectRoot 'docs\FEATURE_IDEAS.md') -Destination $outputPath
    Copy-Item -LiteralPath (Join-Path $projectRoot 'docs\PRODUCTION_GUIDE.md') -Destination $outputPath
    Copy-Item -LiteralPath (Join-Path $projectRoot 'docs\PRODUCTION_FEATURES.md') -Destination $outputPath
    Copy-Item -LiteralPath (Join-Path $projectRoot 'docs\CHARACTERS_GUIDE.md') -Destination $outputPath
    Copy-Item -LiteralPath (Join-Path $projectRoot 'docs\ANIMATION_IMPORT_GUIDE.md') -Destination $outputPath
    $integrationPath = Join-Path $outputPath 'integrations\unreal'
    New-Item -ItemType Directory -Path $integrationPath -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot 'integrations\unreal\export_animations.py') -Destination $integrationPath
    Copy-Item -LiteralPath (Join-Path $projectRoot 'docs\UFBX_LICENSE.txt') -Destination $outputPath
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
    Get-Item -LiteralPath (Join-Path $outputPath 'ReBoneBase-motion.exe') | Select-Object FullName,Length
} finally { Pop-Location }
