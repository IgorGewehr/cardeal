# 20 — Cliente web: estado e plano das telas assíncronas

> ADR-0016. Escrito em 2026-10-01 ao fim da sessão que montou o servidor e o primeiro
> cliente web. **A seção 3 é uma proposta que aguarda decisão** — ela muda as telas do
> desktop, que estão em uso real desde 2026-09-29.

## 1. O que existe

- `cardeal-web`: egui → WASM (WebGL2/glow), mesmo `cardeal-ui` do desktop. Login (cookie
  `HttpOnly`), recarregar sem pedir senha, escolha de empresa, **lista de clientes**, sair.
- Transporte `fetch` sobre o mesmo `cardeal_cliente::remoto::protocolo` do desktop.
- `cargo xtask construir-web` → `dist/web` (versionado por hash, `.br`/`.gz`); o
  `cardeal-server --web` serve com cache imutável e CSP estrita; a imagem Docker já inclui.
- Medido: WASM 1,03 MB em brotli; abertura com cache ~0,25 s; aba ~4 MB acima de uma aba
  vazia (Firefox 156, headless).
- `cargo xtask verificar-wasm` garante que o domínio inteiro (18 crates) continua
  compilando para o navegador sem SQLite.

## 2. O problema

As telas do desktop (`cardeal-desktop/src/tela_*`, ~7,5 mil linhas) chamam o motor de forma
**síncrona** — `motor.consultar(...)` devolve o valor na hora — em 128 lugares. No navegador
nenhuma chamada de rede pode bloquear o quadro. Hoje o desktop resolve o remoto bloqueando
(aceitável numa LAN, perceptível pela internet); o navegador não tem essa opção.

## 3. Proposta: o mesmo código de tela nos três modos

Dois mecanismos, ambos **no-op no desktop local** (o comportamento de hoje não muda):

**Consultas — cache + nova passada (o padrão "suspense").** As funções `carregar` das telas já
são idempotentes (preenchem o estado a partir de consultas). No modo web, `consultar` devolve
o valor do cache se ele existe; senão dispara o `fetch` e devolve um erro `CARREGANDO`
(as telas já tratam `Err` — a `Grade` mostra esqueleto). Quando a resposta chega, o app roda
o `carregar` da área de novo: agora é cache. Consultas independentes saem em paralelo na
mesma passada, de graça. Um comando confirmado invalida o cache.

**Comandos — continuação explícita.** O clique hoje faz `match motor.executar(...) { Ok =>
recarregar/fechar diálogo/notificar, Err => mostrar erro }`. Isso vira
`motor.executar_e(estado, nome, &cmd, |estado, resultado| { …o mesmo match… })`. No local, a
continuação roda na hora; no web, quando a resposta chega. É uma troca mecânica por tela.

**Ordem sugerida**, uma tela por fatia, cada uma testada no desktop e no navegador:
Clientes → OS → Financeiro → Estoque → Agenda → Vendas → Compras → PDV (o PDV por último:
periféricos e o `F8` de supervisor precisam de desenho próprio no navegador).

**O que precisa de decisão:** (a) aprovar o padrão acima; (b) se as telas saem de
`cardeal-desktop` para um crate de telas compartilhado (`cardeal-telas`) usado pelo desktop e
pelo web — recomendado, mantém um binário nativo só, como hoje.

## 4. Pendências conhecidas do cliente web

- Medir a RAM com GPU real (o headless renderiza WebGL em software).
- Fontes de reserva do egui (~1,3 MB brutos) — trocá-las por um recorte de símbolos exige
  decidir o que fazer com emoji digitado pelo usuário (hoje aparece; recortado, viraria `□`).
- Acessibilidade e Ctrl+F do navegador (canvas) — ver "Quando revisitar" na ADR-0016.
