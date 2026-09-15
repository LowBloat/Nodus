# Nodus

Editor desktop nativo em Rust para notas Markdown com sincronização P2P criptografada.

## Sprint 0.3

- arquivos `.md` continuam sendo arquivos comuns, sem formato proprietário;
- editor nativo com tipografia de leitura, busca de notas e visualização CommonMark/GFM;
- `Ctrl+S` salva e inicia o sync; não há gravação nem tentativa periódica em segundo plano;
- rascunhos permanecem na memória ao trocar de nota;
- ao fechar com alterações pendentes, o Nodus oferece salvar tudo, descartar ou cancelar;
- identidade permanente por dispositivo;
- pareamento por convite: cole somente o código do PC1 no PC2 e aprove a solicitação no PC1;
- sincronização bidirecional de criação e alteração iniciada ao salvar;
- conexão QUIC autenticada e criptografada com iroh;
- caminho direto preferido, com relay criptografado como fallback quando o NAT impedir P2P direto;
- cópia de conflito quando uma alteração local mais nova não pode ser substituída com segurança.

O editor preserva o texto Markdown integralmente. A visualização entende CommonMark e recursos GFM como tabelas, listas de tarefas, texto riscado e notas de rodapé. Imagens locais podem ser referenciadas por caminho relativo à nota.

Limites desta sprint: exclusões e renomes ainda não são propagados, não há histórico de versões e conflitos simultâneos ainda são resolvidos por data de modificação com uma cópia de segurança quando necessário.

## Rodar no desenvolvimento

```powershell
cargo run
```

No modo de desenvolvimento, o app cria `notes/` e `nodus-data/` na raiz. O arquivo de identidade em `nodus-data/settings.json` não deve ser copiado entre máquinas.

## Testar em duas máquinas

1. Atualize os dois computadores para a mesma versão e abra o Nodus nos dois.
2. No PC1, copie o código exibido em **Compartilhe seu código**.
3. No PC2, cole esse código e clique em **Adicionar dispositivo**.
4. No PC1, confira o nome e o início do ID apresentado e clique em **Aceitar conexão**.
5. O primeiro sync acontece depois da aprovação. Nas edições seguintes, pressione `Ctrl+S` (ou clique em **Salvar**) para gravar e sincronizar.

Cada cópia cria sua própria pasta `notes` ao lado do executável. Os códigos `NODUS2` contêm a identidade pública, informações de rota do iroh e um token de convite. Eles nunca contêm a chave privada nem o conteúdo das notas, e o convite não concede acesso sem a confirmação no computador que o gerou.

Códigos `NODUS1` das versões anteriores não servem para o novo fluxo de aprovação. Gere e copie um código novo depois de atualizar os dois computadores.

## Privacidade da rede

O iroh tenta migrar a conexão para um caminho direto entre os dispositivos. Quando isso não é possível, usa o relay da infraestrutura padrão como transporte de pacotes criptografados. O relay vê metadados de conexão (como IP, horário e volume aproximado), mas não consegue ler o conteúdo. Uma sprint futura permitirá operar em modo estritamente LAN/direto e usar relay próprio.
