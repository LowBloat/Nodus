# Nodus

Editor desktop nativo em Rust para vaults de notas Markdown com sincronização P2P criptografada.

## Sprint 0.5

- arquivos `.md` continuam comuns e 100% acessíveis por qualquer editor;
- múltiplos vaults, cada um apontando para uma pasta escolhida pelo usuário;
- nenhum vault pode conter outro vault cadastrado;
- lista de vaults, vault ativo, identidade P2P e dispositivos pareados persistem no AppData;
- identidade, código de sync e lista de peers são isolados por vault;
- salvamento automático cerca de 700 ms depois da última edição, seguido de uma tentativa de sync;
- `Ctrl+S` continua disponível para forçar salvamento e sync imediatamente;
- pareamento por convite: cole somente o código do PC1 no PC2 e aprove a solicitação no PC1;
- conexão QUIC autenticada e criptografada com iroh;
- caminho direto preferido, com relay criptografado como fallback quando o NAT impedir P2P direto;
- cópia de conflito quando uma alteração local mais nova não pode ser substituída com segurança.

O editor preserva Markdown puro. A visualização entende CommonMark e recursos GFM como tabelas, listas de tarefas, texto riscado e notas de rodapé. Imagens locais podem ser referenciadas por caminho relativo à nota.

Limites desta sprint: exclusões e renomes ainda não são propagados, vaults inativos não sincronizam em segundo plano e conflitos simultâneos ainda são resolvidos por data de modificação com uma cópia de segurança quando necessário.

## Rodar no desenvolvimento

```powershell
cargo run
```

Na primeira abertura, escolha uma pasta para o vault. O Nodus guarda somente a configuração em `%APPDATA%`; as notas permanecem na pasta escolhida. Instalações portáteis antigas são migradas automaticamente: a antiga pasta `notes/` vira o primeiro vault e `nodus-data/settings.json` é copiado para o AppData.

## Testar em duas máquinas

1. Atualize os dois computadores para a mesma versão e escolha uma pasta de vault em cada um.
2. No PC1, abra **Sync** e copie o código exibido em **Compartilhe seu código**.
3. No PC2, cole esse código e clique em **Adicionar dispositivo**. Se o vault ainda não estiver pareado, ele entra no vault indicado pelo convite.
4. No PC1, confira o nome e o início do ID apresentado e clique em **Aceitar conexão**.
5. O primeiro sync começa depois da aprovação. As edições seguintes são salvas e sincronizadas automaticamente.

Os códigos `NODUS3` contêm a identidade pública, informações de rota do iroh, o identificador do vault e um token de convite. Eles nunca contêm a chave privada nem o conteúdo das notas, e o convite não concede acesso sem a confirmação no computador que o gerou.

Códigos `NODUS1` e `NODUS2` são incompatíveis com o escopo por vault. Gere um código novo depois de atualizar os dois computadores.

## Privacidade da rede

O iroh tenta migrar a conexão para um caminho direto entre os dispositivos. Quando isso não é possível, usa o relay da infraestrutura padrão como transporte de pacotes criptografados. O relay vê metadados de conexão (como IP, horário e volume aproximado), mas não consegue ler o conteúdo. Uma sprint futura permitirá operar em modo estritamente LAN/direto e usar relay próprio.
