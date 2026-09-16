# release.ps1 - empacota uma release do Nodus em dist/.
#
# Uso (a partir da raiz do projeto):
#     .\release.ps1
#
# Pre-requisitos:
#     cargo build --release   # para gerar target/release/nodus.exe
#     packaging/LEIA-ME.md presente no repositorio
#
# Saida:
#     dist/Nodus-<versao>-portable/        pasta portatil com todos os arquivos
#     dist/Nodus-<versao>-windows-x64.zip  mesmo conteudo zipado

$ErrorActionPreference = "Stop"

# Vai para o diretorio do script (a raiz do projeto) antes de qualquer coisa.
Set-Location $PSScriptRoot

# --- 1. Versao a partir do Cargo.toml ----------------------------------------

$cargoToml = Get-Content "Cargo.toml" -Raw
if ($cargoToml -notmatch '(?m)^\s*version\s*=\s*"([^"]+)"') {
    throw "Nao foi possivel ler a versao de Cargo.toml"
}
$version = $Matches[1]
Write-Host "Versao detectada: $version" -ForegroundColor Cyan

# --- 2. Binario do release ---------------------------------------------------

$exePath = Join-Path "target/release" "nodus.exe"
if (-not (Test-Path $exePath)) {
    throw "Binario nao encontrado em '$exePath'. Rode 'cargo build --release' antes."
}

# --- 3. Caminhos de saida ----------------------------------------------------

$portableDir = Join-Path "dist" "Nodus-$version-portable"
$zipPath = Join-Path "dist" "Nodus-$version-windows-x64.zip"
# --- 4. Limpa e recria a pasta ------------------------------------------------

if (Test-Path $portableDir) {
    Remove-Item $portableDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $portableDir | Out-Null

# --- 5. Copia os arquivos ----------------------------------------------------

Copy-Item $exePath (Join-Path $portableDir "Nodus.exe")
Copy-Item "packaging/LEIA-ME.md" $portableDir
Copy-Item "assets/fonts/Inter-LICENSE.txt" $portableDir
Copy-Item "assets/fonts/SourceSerif4-LICENSE.md" $portableDir

# --- 6. Zip -------------------------------------------------------------------

if (Test-Path $zipPath) {
    Remove-Item $zipPath -Force
}
Compress-Archive -Path $portableDir -DestinationPath $zipPath

# --- 7. Resumo ---------------------------------------------------------------

Write-Host ""
Write-Host "Release empacotada:" -ForegroundColor Green
Write-Host "    Pasta: $portableDir"
Write-Host "    Zip:   $zipPath"
