# release.ps1 - empacota uma release do Nodus em dist/.
#
# Uso (a partir da raiz do projeto):
#     .\release.ps1
#
# Pre-requisitos:
#     cargo build --release   # para gerar target/release/nodus.exe
#     LEIA-ME.md presente em dist/Nodus-<versao>-portable/  (crie a partir
#     do da ultima release se for uma nova versao)
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
$readmePath = Join-Path $portableDir "LEIA-ME.md"

# --- 4. LEIA-ME: obrigatorio. Se faltar, semeia do anterior -----------------

if (-not (Test-Path $readmePath)) {
    $latest = Get-ChildItem "dist" -Filter "LEIA-ME.md" -Recurse -ErrorAction SilentlyContinue |
        Where-Object { ($_.DirectoryName | Split-Path -Leaf) -ne (Split-Path $portableDir -Leaf) } |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if ($latest) {
        Write-Host "Semendo LEIA-ME.md a partir de $($latest.FullName) - edite antes de publicar." -ForegroundColor Yellow
        New-Item -ItemType Directory -Force -Path $portableDir | Out-Null
        Copy-Item $latest.FullName $readmePath
    } else {
        throw "Nenhum LEIA-ME.md existente em dist/. Crie '$readmePath' manualmente primeiro."
    }
}

# --- 5. Limpa a pasta (mas preserva o LEIA-ME.md) -----------------------------

if (Test-Path $portableDir) {
    Get-ChildItem $portableDir -Force |
        Where-Object { $_.Name -ne "LEIA-ME.md" } |
        Remove-Item -Recurse -Force
}
$notesInsidePortable = Join-Path $portableDir "notes"
New-Item -ItemType Directory -Force -Path $notesInsidePortable | Out-Null

# --- 6. Copia os arquivos ----------------------------------------------------

Copy-Item $exePath (Join-Path $portableDir "Nodus.exe")
Copy-Item "assets/fonts/Inter-LICENSE.txt" $portableDir
Copy-Item "assets/fonts/SourceSerif4-LICENSE.md" $portableDir
Copy-Item "notes/Bem-vindo.md" $notesInsidePortable

# --- 7. Zip -------------------------------------------------------------------

if (Test-Path $zipPath) {
    Remove-Item $zipPath -Force
}
Compress-Archive -Path $portableDir -DestinationPath $zipPath

# --- 8. Resumo ---------------------------------------------------------------

Write-Host ""
Write-Host "Release empacotada:" -ForegroundColor Green
Write-Host "    Pasta: $portableDir"
Write-Host "    Zip:   $zipPath"
