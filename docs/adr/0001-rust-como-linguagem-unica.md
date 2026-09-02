# ADR-0001 — Rust como linguagem única do sistema

- **Status:** Aceita
- **Data:** 2026-09-01
- **Decisores:** Equipe de arquitetura
- **Relacionadas:** ADR-0004, ADR-0009, ADR-0010

## Contexto

O Cardeal roda em três lugares diferentes com a mesma exigência de eficiência: o servidor na sala
dos fundos de uma loja, o terminal de PDV no balcão e o desktop monoposto de um MEI que também
mantém o navegador aberto. Os três têm o mesmo orçamento apertado de RAM e CPU descrito no
[doc 02](../02-pilar-eficiencia.md) — 40 a 180 MB conforme o cenário, boot abaixo de 800 ms, CPU em
repouso menor que 0,5%. Esse orçamento não é aspiracional: é testado em CI e o build falha se for
estourado.

Historicamente, ERP de PME brasileiro se divide entre dois mundos. De um lado, pilhas gerenciadas
(Delphi, que ainda domina uma fatia grande do mercado nacional, .NET/WinForms, Java/Swing) que
pagam o preço de coletor de lixo e runtime instalado no cliente. Do outro, pilhas web empacotadas em
Electron, que resolvem produtividade de UI trazendo consigo um Chromium inteiro — e o consumo de RAM
que vem junto. Nenhuma das duas cumpre o orçamento do doc 02 sem reescrever a camada de execução por
baixo.

Há também uma força específica do domínio: o núcleo financeiro ([doc 05](../05-nucleo-financeiro.md))
modela estados que precisam ser exaustivos e impossíveis de representar incorretamente —
`EstadoLancamento`, a máquina de estados fiscal do [doc 10](../10-modulo-fiscal.md), o
`LancamentoBalanceado` que só existe se a soma das partidas fechar em zero. Um sistema de tipos com
soma (enums fechados, `match` exaustivo verificado em compilação) reduz uma classe inteira de bugs
contábeis a erro de compilação, em vez de bug de produção descoberto pelo contador seis meses depois.

Por fim, há uma força organizacional: a equipe é pequena e vai crescer aos poucos. Um único idioma
para servidor, UI, ferramentas de build (`xtask`) e testes significa uma única cadeia de ferramentas,
um único `cargo build`, um único conjunto de convenções de erro e concorrência — sem a fricção de
contratar separadamente "quem sabe backend" e "quem sabe frontend" nem de manter dois `CI` distintos.

## Alternativas consideradas

| Critério | **Rust** | C#/.NET | Java | Go | Delphi | TypeScript/Electron |
|---|---|---|---|---|---|---|
| RAM em repouso (app comparável) | 2–90 MB | ~260 MB (WinForms+ORM) | ~700 MB (Swing) | ~35 MB | ~120 MB | ~480 MB |
| Pausas de GC | nenhuma | 10–50 ms | 10–100 ms | 1–10 ms (baixa) | nenhuma (ARC) | 10–50 ms (V8) |
| Runtime a instalar no cliente | não (binário estático) | sim (.NET runtime) | sim (JVM) | não | não | sim (Chromium/Node embutido) |
| Tipos soma exaustivos (`match`) | sim, nativo | parcial (`switch` não exaustivo) | não | não (sem enums de dados) | não | parcial (union types, não verificado em runtime) |
| Segurança de memória sem GC | sim (borrow checker) | GC | GC | GC | ARC + `unsafe` comum | GC |
| Mercado de contratação BR (Rust vs. o resto) | pequeno, crescendo | grande | grande | médio | grande (legado) | muito grande |
| Binário único, sem dependência externa | sim | não (redistribuível .NET) | não (JRE) | sim | quase (BPL opcional) | não |
| Maturidade do ecossistema de UI desktop nativa | média (egui, iced, Slint) | alta (WinForms/WPF) | média | baixa | alta (VCL) | alta (web) |

Delphi merece nota à parte: é a opção "dominante" no ERP brasileiro por tradição — grande parte dos
sistemas legados do setor foi escrita nele. Descartamos por três razões concretas: o pool de
desenvolvedores jovens está encolhendo, não há sistema de posse (ownership) que prove ausência de
`use-after-free` em tempo de compilação, e o ecossistema de bibliotecas modernas (serialização,
async, criptografia) é comparativamente pobre.

## Decisão

**Rust é a única linguagem de implementação do sistema — servidor, núcleo de domínio, interface
desktop, ferramentas internas (`xtask`) e testes — sem exceção por conveniência.**

Todo o workspace compila com o mesmo `cargo`, os mesmos lints (`clippy::pedantic`), o mesmo
`rustfmt`. Não há um segundo idioma "só para a UI" nem "só para scripts de build". Onde precisamos de
FFI (drivers de periférico, criptografia de baixo nível), isolamos `unsafe` em módulos marcados e
justificados — nunca escrevemos um binário auxiliar em outra linguagem para contornar uma limitação
momentânea. A ausência de coletor de lixo é o que sustenta o alvo de latência do PDV: menos de 400 ms
entre `F2` e o cupom impresso, sem depender de uma pausa de GC não acontecer no pior momento
possível — durante a fila do almoço.

## Consequências

### Positivas

- RAM e CPU dentro do orçamento do [doc 02](../02-pilar-eficiencia.md) sem otimização heroica: o
  runtime não paga alocação de objeto gerenciado nem pausa de coleta.
- Latência previsível: o percentil 99 de uma operação não é surpreendido por um GC no pior momento
  possível (fila no caixa, hora de pico).
- O sistema de tipos codifica invariantes de negócio (`LancamentoBalanceado`, estados fiscais) de
  forma que o compilador rejeita programas incorretos antes de chegarem a testes.
- Um binário estático por plataforma, sem runtime a instalar no PC do cliente — coerente com o
  princípio "configuração é exceção" do [doc 00 §4](../00-visao-produto.md).
- Uma única cadeia de ferramentas (`cargo fmt`, `cargo clippy`, `cargo test`, `cargo xtask`) para
  toda a equipe, reduzindo a superfície de "como isso se builda" a uma resposta.
- `#![forbid(unsafe_code)]` como padrão em quase todo crate torna a segurança de memória uma
  propriedade verificada, não uma disciplina de revisão.

### Negativas

- O mercado de contratação de Rust no Brasil é sensivelmente menor que o de C#, Java ou JavaScript —
  vagas levam mais tempo para fechar e o salário de mercado tende a ser mais alto para sêniores.
- A curva de aprendizado do borrow checker é real para quem nunca programou com posse explícita;
  os primeiros PRs de um desenvolvedor novo costumam ter idas e vindas de compilação antes de
  passarem.
- Tempo de compilação do workspace completo (~4 minutos na primeira vez, por medição do
  [doc 16](../16-onboarding.md)) é maior que o de uma stack interpretada, o que pesa no ciclo de
  iteração se a disciplina de `cargo check` incremental não for adotada.
- Bibliotecas de nicho (drivers de periféricos exóticos de PDV, alguns protocolos regionais de TEF)
  às vezes só existem maduras em C ou em SDKs proprietários — exigindo FFI justificada.

### Mitigações

- Onboarding estruturado para produtividade em 4 horas e entrega de um módulo simples em uma semana
  ([doc 16](../16-onboarding.md)) — o próprio documento é a mitigação da curva de aprendizado.
- O domínio de negócio é deliberadamente síncrono e simples ([doc 02 §2.5](../02-pilar-eficiencia.md)):
  `cardeal-kernel` e `cardeal-ledger` não usam `async`, o que remove a parte mais difícil do
  ecossistema Rust (futures, lifetimes em código assíncrono) do caminho que 70% dos desenvolvedores
  tocam no dia a dia.
- Convenção de baixa genericidade: preferimos `enum` e `dyn Trait` explícitos a hierarquias
  genéricas profundas, o que mantém mensagens de erro do compilador legíveis.
- `cargo` é o único build tool do projeto — não há Make, não há script de shell paralelo escondendo
  passos, o que reduz a superfície do "como isso builda" que um novo integrante precisa aprender.
- Para hiring, investimos em treinar internamente desenvolvedores de C#/Java com boa base
  algorítmica em vez de depender só de contratação de Rust sênior pronta — a curva de 4h/1 semana do
  onboarding torna essa aposta viável.

## Quando revisitar

- Se o tempo médio de um desenvolvedor novo para entregar seu primeiro módulo simples ultrapassar
  consistentemente (mais de 50% dos casos, em três contratações seguidas) o prazo de uma semana
  prometido no [doc 16](../16-onboarding.md).
- Se o tempo de build completo do workspace ultrapassar 15 minutos em hardware de desenvolvedor
  padrão, degradando o ciclo de iteração a ponto de a equipe recorrer a builds parciais como
  hábito permanente (sintoma de que a modularização em crates parou de funcionar).
- Se, por dois trimestres seguidos, vagas de Rust sênior ficarem abertas por mais de 90 dias sem
  candidato qualificado, tornando a contratação o gargalo real do roadmap.
- Se surgir uma dependência crítica (driver de periférico obrigatório, SDK de certificação fiscal)
  disponível apenas como binário para outra linguagem sem caminho de FFI viável.
- Nenhum desses gatilhos, isoladamente, justifica abandonar Rust — a decisão seria reavaliada apenas
  se dois ou mais ocorrerem simultaneamente, porque o custo de reescrever um sistema deste porte em
  outra linguagem é, ele mesmo, maior que qualquer um dos problemas acima isoladamente.
