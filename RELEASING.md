# Publicar uma versão do Nodus

Uma GitHub Release é uma página de download ligada a uma tag Git. A tag marca exatamente qual commit originou os executáveis; os arquivos `.zip` e `.tar.gz` são os artefatos instaláveis anexados à página.

## Plataformas geradas

- Windows x64, x86 (32 bits) e ARM64;
- Linux x64, x86 (32 bits) e ARM64.

Cada pacote recebe também um arquivo `.sha256`. O workflow só publica a Release depois que todas as seis compilações terminam com sucesso.

## Criar uma nova Release

1. Atualize `version` em `Cargo.toml` e confirme que `Cargo.lock` recebeu a mesma versão.
2. Atualize o README e as notas relevantes da versão.
3. Rode localmente:

   ```powershell
   cargo fmt -- --check
   cargo clippy --all-targets -- -D warnings
   cargo test
   ```

4. Faça commit e push das mudanças.
5. Crie e envie uma tag com a mesma versão do `Cargo.toml`:

   ```powershell
   git tag -a v0.6.0 -m "Nodus v0.6.0"
   git push origin v0.6.0
   ```

6. Acompanhe **Actions → Release multiplataforma** no GitHub. Quando todas as compilações passarem, a página **Releases** será criada automaticamente com os downloads.

## Repetir uma compilação

Se houver uma falha de infraestrutura, abra **Actions → Release multiplataforma → Run workflow**, informe uma tag já existente e execute novamente. Pacotes com o mesmo nome são substituídos na Release.

Não mova uma tag de uma versão já publicada. Para mudanças no aplicativo, incremente a versão e crie uma tag nova.
