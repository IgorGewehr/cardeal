# ADR-0004 — egui/eframe como camada de interface

- **Status:** Aceita
- **Data:** 2026-09-01
- **Decisores:** Equipe de arquitetura
- **Relacionadas:** ADR-0001, ADR-0014

## Contexto

A interface do Cardeal roda no mesmo hardware modesto do restante do sistema: o terminal de PDV tem
orçamento de 90 MB de RSS e 0% de CPU ociosa ([doc 02](../02-pilar-eficiencia.md)). Ao mesmo tempo, a
tela principal do produto — o Pulso ([doc 12 §6](../12-ui-ux.md)) — é um gráfico contínuo e
interativo (o Rio do Caixa), e as telas de retaguarda precisam de grades densas com dezenas de
milhares de linhas, redimensionamento de coluna e virtualização eficiente
([doc 02 §2.3](../02-pilar-eficiencia.md)). São exigências de UI de ferramenta de trabalho, não de
site institucional.

A opção mais comum do mercado para chegar rápido a uma UI bonita é empacotar uma aplicação web dentro
de um shell nativo (Electron ou Tauri com frontend web). Isso traz produtividade de UI e um mercado
de contratação enorme (qualquer desenvolvedor web serve), mas paga o preço de rodar um motor de
renderização de página inteiro — e esse preço aparece diretamente no orçamento de RAM que é um dos
três pilares do produto.

Rust já é a linguagem única do sistema ([ADR-0001](0001-rust-como-linguagem-unica.md)), então as
opções de UI nativa em Rust entram como candidatas naturais: `egui`, `iced`, `Slint`, `Dioxus
desktop`. A pergunta não é "qual framework de UI é mais bonito", mas "qual entrega grades densas,
formulários complexos e um gráfico interativo customizado (o Rio do Caixa) dentro do orçamento de
RAM, sem exigir que construamos um motor de renderização do zero".

## Alternativas consideradas

| Critério | **egui/eframe** | Tauri + web | Electron | iced | Slint | Dioxus desktop |
|---|---|---|---|---|---|---|
| RAM em uso típico (app comparável) | 40–90 MB | ~150–250 MB (webview do SO) | ~300–450 MB (Chromium embutido) | ~60–100 MB | ~50–90 MB | ~120–200 MB (WebView) |
| CPU ociosa (modo reativo) | 0% (`wait_events`) | próximo de 0% (webview idle) | próximo de 0%, mas overhead de processo maior | 0% (também reativo) | 0% | próximo de 0% |
| Tempo de inicialização | ~200 ms | ~400–600 ms (webview) | ~800 ms–1,5 s | ~250 ms | ~200 ms | ~500 ms |
| Produtividade em grades densas e customizadas | alta (modo imediato, controle total de layout) | alta (HTML/CSS/JS maduro) | alta (idem) | média (requer mais código por widget) | média-alta (linguagem de markup própria) | média (JSX-like, mas ecossistema jovem) |
| Ecossistema de componentes prontos | pequeno, cresce | enorme (todo o npm) | enorme | pequeno | pequeno-médio | pequeno |
| Estilo (CSS ou equivalente) | não tem — layout é código | CSS completo | CSS completo | sistema de estilo próprio, limitado | linguagem de estilo própria (.slint) | CSS via web |
| Mercado de contratação | Rust (pequeno) | web (enorme) + Rust (pequeno) | web (enorme) | Rust (pequeno) | Rust (pequeno) | Rust + React-like (pequeno) |
| Maturidade/estabilidade da API | média-alta, ativamente mantida | alta | muito alta | média | média | baixa-média |

O comparativo de RAM medido em bancada, já citado no [doc 02 §1](../02-pilar-eficiencia.md), resume o
argumento decisivo: um ERP típico em Electron consome ≈480 MB para a mesma carga em que o Cardeal
mira 90 MB no terminal de PDV. Essa diferença por si só inviabiliza Electron/Tauri-web para o
hardware alvo do produto (PCs de balcão de PME, muitas vezes com 4 GB de RAM total, compartilhados
com o navegador do dono).

## Decisão

**A interface desktop do Cardeal é construída em Rust puro sobre `egui`/`eframe`, com backend
`wgpu`, e o design system "Rubro" é implementado como uma camada de componentes própria sobre esse
modo imediato.**

Modo imediato (`immediate mode`) significa que a UI é redesenhada como função do estado a cada quadro
solicitado — sem árvore de widgets retida, sem reconciliação — e que o processo fica bloqueado em
`wait_events` (0% de CPU) até que algo realmente mude: interação do usuário, evento do servidor ou
uma animação com prazo declarado ([doc 02 §2.1](../02-pilar-eficiencia.md)). Isso é o que torna
grades densas, o Rio do Caixa e a paleta de comandos triviais de implementar sem sacrificar consumo
de energia. Detalhes de paleta, tipografia e componentes estão em [doc 12](../12-ui-ux.md).

## Consequências

### Positivas

- RAM de 40–90 MB contra 300–450 MB de uma alternativa baseada em webview — dentro do orçamento do
  terminal de PDV sem margem de risco.
- 0% de CPU ociosa real: sem timer de 60 fps, sem processo de renderização em segundo plano
  consumindo bateria de nobreak.
- Um só idioma e um só sistema de build para toda a equipe — a UI não é um projeto separado com
  `package.json`, `node_modules` e uma cadeia de ferramentas JavaScript paralela.
- Modo imediato é naturalmente produtivo para grades densas e dinâmicas: não há necessidade de
  gerenciar reconciliação de árvore de componentes para uma tela que muda a cada tecla digitada no
  PDV.
- Tokens de design são tipos Rust, não strings CSS — usar a cor errada é erro de compilação, não bug
  visual descoberto em produção (`Botao::primario(...)`, `Valor::dinheiro(...)`).
- Tempo de inicialização de ~200 ms contribui diretamente para o alvo de boot do terminal
  (< 400 ms, [doc 02](../02-pilar-eficiencia.md)).

### Negativas

- Não existe CSS: todo layout, espaçamento e estado visual é código Rust — o design system precisa
  ser construído inteiramente por nós, sem herdar nada do ecossistema web.
- Ecossistema de componentes prontos é pequeno comparado ao de qualquer framework web: coisas triviais
  em HTML (um seletor de data com calendário, um rich text editor) exigem implementação própria.
- Acessibilidade nativa do sistema operacional (leitor de tela, navegação por Narrator/JAWS) é mais
  limitada que em UIs que usam controles nativos do SO ou HTML semântico.
- Texto complexo — bidirecional (RTL), alguns scripts CJK com quebra de linha sofisticada, `IME` para
  entrada de texto asiático — é uma área historicamente mais fraca do `egui` que de motores de
  renderização de texto maduros (Skia, DirectWrite, HarfBuzz completo).
- Contratação de desenvolvedores com experiência prévia em `egui` é praticamente inexistente no
  mercado — todo mundo aprende no projeto.

### Mitigações

- O crate `cardeal-ui` centraliza todo o design system Rubro: tokens tipados, tipografia tabular,
  componentes de grade densa, campo monetário, seletor de conta — um único lugar de verdade para
  cor, espaçamento e tipografia ([doc 01 §8](../01-arquitetura-geral.md), [doc 12](../12-ui-ux.md)).
- Uma galeria viva de componentes (`cargo run -p cardeal-ui --example galeria`) documenta e testa
  visualmente cada peça do design system antes de ela ser usada em uma tela real.
- Testes de captura de tela (`egui` headless comparado a imagem de referência, falha acima de 0,3% de
  diferença) e testes de contraste WCAG AA automatizados em CI substituem parte da revisão manual que
  um design system maduro de mercado teria de fábrica ([doc 14 §7](../14-testes-qualidade.md)).
- O escopo do produto (mercado brasileiro, sem exigência de internacionalização — ver
  [doc 00 §5](../00-visao-produto.md)) reduz a superfície do problema de texto complexo: não há
  requisito de RTL nem de scripts CJK na v1.
- Um console web (Leptos, reutilizando os mesmos tokens exportados como CSS custom properties) está
  previsto no roadmap para acesso remoto — não como substituto do desktop, mas como complemento onde
  a acessibilidade web nativa é relevante ([doc 12 §10](../12-ui-ux.md)).

## Quando revisitar

- Se a meta de acessibilidade do produto crescer para exigir suporte pleno de leitor de tela e o
  `egui` não evoluir suporte nativo suficiente — hoje o requisito de acessibilidade do
  [doc 12 §9](../12-ui-ux.md) é contraste e navegação por teclado, não leitor de tela completo.
- Se um cliente com necessidade real de texto bidirecional ou de scripts complexos aparecer,
  contrariando a premissa de escopo nacional do [doc 00 §5](../00-visao-produto.md).
- Se o custo de manter o design system Rubro internamente (medido em horas de engenharia por
  trimestre gastas em componentes de UI versus funcionalidade de negócio) ultrapassar
  consistentemente o que uma migração para uma stack com ecossistema maduro economizaria.
- Se o `egui` parar de receber manutenção ativa upstream por um período prolongado (mais de 6 meses
  sem release) e uma vulnerabilidade ou limitação bloqueante não puder ser corrigida internamente.
- Não revisitaríamos por "faltam componentes prontos" isoladamente — esse é um custo conhecido e
  aceito, mitigado pelo crescimento incremental do `cardeal-ui`.
