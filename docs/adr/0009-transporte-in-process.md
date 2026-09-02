# ADR-0009 — Transporte em memória no modo monoposto

- **Status:** Aceita
- **Data:** 2026-09-01
- **Decisores:** Equipe de arquitetura
- **Relacionadas:** ADR-0001, ADR-0003, ADR-0008

## Contexto

O Cardeal atende três modos de implantação com o mesmo binário: monoposto (um MEI, um único
processo), rede local (comércio típico, servidor mais alguns PDVs) e multi-loja
([doc 01 §6](../01-arquitetura-geral.md)). No modo monoposto, cliente e motor rodam no mesmo processo
— não há razão de arquitetura para o comando `FinalizarVenda` sair da thread da UI, virar bytes,
atravessar um socket loopback e voltar a ser um struct do outro lado, quando o despachante que o
processaria está a uma chamada de função de distância.

Ao mesmo tempo, o restante do sistema é construído sobre a premissa de que existe uma "rede" entre
cliente e servidor: autorização acontece no servidor ([doc 01 §3](../01-arquitetura-geral.md)),
comandos carregam chave de idempotência para sobreviver a reenvio ([doc 03 §2.2](../03-pilar-resiliencia.md)),
o protocolo é versionado ([doc 09 §7](../09-protocolo-api.md)). Reescrever essa lógica de negócio para
um "modo local" diferente do "modo rede" duplicaria a superfície de bugs exatamente no caminho mais
crítico do sistema — o caminho de uma venda.

A solução observada no [doc 01 §6](../01-arquitetura-geral.md) e no [doc 09 §4](../09-protocolo-api.md)
é: manter uma única abstração de transporte (o mesmo `trait` que envia um envelope de comando e
recebe uma resposta), e dar a essa abstração duas implementações — uma que serializa e atravessa
socket (usada em LAN e em integrações externas), e uma que apenas invoca o despachante diretamente em
memória, sem socket nem serialização, quando o motor está embutido no mesmo processo do desktop. Essa
segunda implementação é o que permite o MEI rodar em ~55 MB e com um único processo
([doc 01 §6](../01-arquitetura-geral.md)) — sem essa otimização, o modo monoposto pagaria o custo de
dois processos e um round-trip de rede local para uma operação que não precisa disso.

## Alternativas consideradas

| Critério | **Transporte in-process** | Sempre socket loopback (mesmo em monoposto) | Dois códigos de domínio (local vs. rede) | Biblioteca compartilhada sem processo separado, sem abstração de transporte |
|---|---|---|---|---|
| Latência do caminho de comando | ~5 µs (chamada direta) | ~800 µs (loop de rede + serialização, mesmo em loopback) | ~5 µs no caminho local, mas duplicado | ~5 µs |
| RAM adicional (processo extra) | nenhuma | um processo a mais rodando, ainda que loopback | nenhuma | nenhuma |
| Risco de divergência de comportamento | existe, mitigado por suíte dupla | não existe (sempre o mesmo caminho) | alto — dois códigos para manter sincronizados | alto — sem abstração, fácil "vazar" suposições de rede no domínio |
| Reaproveitamento de autorização, idempotência, protocolo | total (mesmo despachante) | total | duplicado, risco de divergir | parcial, arriscado |
| Complexidade de manutenção | média (uma abstração, dois testes) | baixa (um só caminho) | alta (dois caminhos completos) | baixa no curto prazo, alta no médio (acoplamento cresce) |
| Coerência com o alvo de RAM do MEI (< 110 MB monoposto, [doc 02](../02-pilar-eficiencia.md)) | sim | compromete (overhead de dois processos + pilha de rede) | sim | sim |

Manter sempre o socket loopback, mesmo em monoposto, foi a alternativa mais próxima de ser aceita —
tem a vantagem de eliminar por completo o risco de divergência entre os dois transportes. Foi
descartada porque o custo (latência de ~800 µs por operação, mais o overhead de dois processos e uma
pilha de rede sempre ativa) não se justifica quando a mesma garantia de comportamento pode ser obtida
por disciplina de teste, a um custo de latência 160× menor.

## Decisão

**O mesmo `trait` de transporte tem duas implementações — uma que serializa e envia por rede
(HTTP/2 sobre mTLS em LAN, WebSocket para tempo real), e uma que despacha diretamente em memória sem
socket nem serialização — selecionada em tempo de boot conforme o modo de implantação.**

Em monoposto, o motor sobe como thread dentro do próprio processo do desktop e o cliente usa a
implementação em memória do transporte: o comando chega ao despachante como uma chamada de função
direta, sem passar por `postcard` nem por um socket ([ADR-0008](0008-postcard-como-serializacao-interna.md)).
A [tabela de transporte do doc 09 §4](../09-protocolo-api.md) documenta os dois caminhos lado a lado:
~5 µs para o modo monoposto contra ~0,8 ms para LAN — mesma API, latências de ordem de grandeza
diferentes.

## Consequências

### Positivas

- Latência do caminho de comando cerca de 160× menor no modo monoposto (~5 µs contra ~800 µs) —
  imperceptível ao usuário em qualquer um dos dois casos, mas relevante para o orçamento de CPU total
  em sequências de operações encadeadas.
- Nenhum processo adicional, nenhuma pilha de rede ativa (nem em loopback) no modo monoposto — parte
  direta do motivo pelo qual o MEI roda em ~55 MB de RSS.
- Toda a lógica de negócio (autorização, validação, escrita) é exatamente a mesma nos dois modos —
  não existe um "modo simplificado" do domínio que se comporte diferente.
- A abstração de transporte único mantém o resto do sistema (protocolo, idempotência, autorização)
  agnóstico a como o envelope efetivamente viaja — o despachante não sabe, nem precisa saber, se foi
  chamado por rede ou em memória.

### Negativas

- Dois caminhos de código de transporte precisam ser testados, não um — qualquer suíte de teste que
  cubra só um dos dois deixa uma lacuna real de comportamento não verificado.
- Risco concreto de divergência de comportamento entre os dois caminhos: por exemplo, um erro de
  serialização que só aparece quando o envelope realmente atravessa `postcard` e a rede nunca
  aparecerá no caminho em memória, e vice-versa (um bug de concorrência entre threads pode só aparecer
  no caminho que efetivamente cruza um limite de processo).
- A implementação em memória precisa reproduzir fielmente qualquer efeito colateral que o transporte
  de rede teria (por exemplo, aplicar timeouts e limites de tamanho de payload de forma equivalente),
  ou o comportamento observável do sistema passa a depender do modo de implantação — o que viola a
  premissa de "mesmo binário, mesmo comportamento" do produto.

### Mitigações

- A suíte de integração roda **todos** os testes relevantes de comando nos dois transportes — não é
  uma suíte espelhada mantida à parte, é a mesma bateria de testes parametrizada pelo transporte, o
  que torna impossível esquecer de rodar um caminho ao adicionar um teste novo.
- A serialização com `postcard` é exercitada explicitamente mesmo em cenários de teste do caminho em
  memória sempre que o teste for especificamente sobre compatibilidade de protocolo — separando "teste
  de comportamento de negócio" (roda nos dois transportes) de "teste de serialização" (roda sempre
  contra o formato real).
- O teste de queda de energia e os cenários end-to-end ([doc 03 §7](../03-pilar-resiliencia.md),
  [doc 14 §6](../14-testes-qualidade.md)) incluem cenários de MEI rodando em modo monoposto, cobrindo
  o caminho em memória sob a mesma disciplina de verificação que cobre o modo em rede.

## Quando revisitar

- Se a suíte dupla (mesmo teste, dois transportes) revelar divergência de comportamento em produção
  mais de uma vez em um trimestre — sinal de que a promessa de "mesma lógica, dois transportes" não
  está sendo mantida na prática e a abstração precisa de reforço (por exemplo, um `harness` de
  verificação de equivalência automática entre os dois caminhos, não apenas testes espelhados).
- Se o modo monoposto vier a precisar de propriedades que só um transporte de rede real oferece
  naturalmente (por exemplo, algum tipo de isolamento de processo por segurança) — o que mudaria o
  cálculo de custo-benefício da chamada direta.
- Se o tempo de execução da suíte dupla se tornar um gargalo de CI relevante (por exemplo, dobrar o
  tempo total de testes de forma perceptível), justificando revisar a estratégia de parametrização em
  vez de execução completa duplicada.
- Não revisitaríamos por "é mais um caminho de código a manter" isoladamente — esse custo é conhecido,
  pequeno em superfície (a implementação em memória é uma chamada de função) e a suíte dupla é a
  mitigação desenhada especificamente para ele.
