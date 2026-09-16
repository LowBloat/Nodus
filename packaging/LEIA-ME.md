# Nodus

Editor desktop nativo de notas Markdown com sincronização P2P criptografada.

## Escolha do pacote

- `windows-x64`: computadores Windows comuns de 64 bits;
- `windows-x86`: Windows de 32 bits;
- `windows-arm64`: dispositivos Windows com processador ARM;
- `linux-x64`: distribuições Linux em computadores Intel/AMD de 64 bits;
- `linux-x86`: distribuições Linux de 32 bits;
- `linux-arm64`: Linux em ARM64, incluindo vários notebooks e placas recentes.

## Primeira abertura

No Windows, abra `Nodus.exe`. No Linux, extraia o pacote, permita a execução se necessário e abra `Nodus`:

```bash
chmod +x Nodus
./Nodus
```

Escolha uma pasta para o vault. Suas notas permanecem como arquivos `.md` comuns dentro dessa pasta. Configurações, identidades e dispositivos confiáveis ficam na pasta de dados do usuário, nunca ao lado do executável.

## Sincronizar duas máquinas

1. Escolha uma pasta de vault nas duas máquinas.
2. Na primeira máquina, abra **Sincronização** e copie o convite.
3. Na segunda, cole o convite e clique em **Adicionar dispositivo**.
4. Na primeira, confira o dispositivo e aceite a conexão.

O primeiro sync começa após a aprovação. Depois disso, salvar inicia uma nova tentativa de sincronização. `Ctrl+S` força a gravação e o sync imediatamente.

## Atalhos

- `Ctrl+B`: mostrar ou ocultar a sidebar;
- `Ctrl+/`: buscar notas;
- `Ctrl+E`: alternar entre escrever, dividir e ler;
- `Ctrl+Shift+P`: mostrar ou ocultar a sincronização;
- `Ctrl+S`: salvar e sincronizar agora.

## Limites atuais

- exclusões e renomes ainda não são propagados;
- somente o vault ativo sincroniza;
- alterações simultâneas podem gerar uma cópia de conflito para preservar os dois conteúdos.

Cada arquivo compactado possui um `.sha256` correspondente na página da Release para verificação de integridade.
