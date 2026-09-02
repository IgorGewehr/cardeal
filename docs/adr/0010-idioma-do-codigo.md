# ADR-0010 — Domínio em português, infraestrutura em inglês

- **Status:** Aceita
- **Data:** 2026-09-01
- **Decisores:** Equipe de arquitetura
- **Relacionadas:** ADR-0001, ADR-0002

## Contexto

O Cardeal é um ERP para o mercado brasileiro, com escopo deliberadamente nacional — sem
internacionalização prevista ([doc 00 §5](../00-visao-produto.md)). A equipe conversa com clientes,
contadores e usuários finais em português, e boa parte do vocabulário do domínio não tem tradução
limpa para o inglês: "competência" (regime contábil), "sangria" (retirada de caixa), "encerrante"
(contador de bomba de combustível), "CFOP" (classificação fiscal), "fôlego" (termo do próprio produto
para dias de caixa restantes) são conceitos do direito contábil, da legislação fiscal ou do
vocabulário do próprio produto brasileiro. Traduzir `competencia` para `accrual_date` no código não
elimina a necessidade de o desenvolvedor pensar em português ao conversar com o cliente ou ao ler o
[glossário](../18-glossario.md) — apenas insere uma camada de tradução mental em toda revisão de
código, toda especificação de módulo e toda conversa técnica sobre uma regra de negócio.

Ao mesmo tempo, Rust — como toda a infraestrutura de bibliotecas do ecossistema, as convenções de
`trait`, os nomes exigidos por `Iterator`, `Display`, `From` — é irredutivelmente em inglês. Forçar
tradução de nomes de trait de biblioteca padrão ou de tipos genéricos (`T`, `E`) não ganharia nada e
quebraria a legibilidade para quem vem de fora, além de brigar com o próprio compilador e com
ferramentas de terceiros que esperam convenções inglesas.

A pergunta arquitetural, então, não é "português ou inglês", mas onde exatamente passa a fronteira —
e essa fronteira precisa ser simples o bastante para ser seguida sem ambiguidade em cada arquivo novo,
porque a alternativa (deixar cada desenvolvedor escolher por conta própria) produz uma base de código
inconsistente, que é o pior dos dois mundos.

## Alternativas consideradas

| Critério | **Domínio PT / infra EN** | Tudo em inglês | Tudo em português | Inglês com glossário bilíngue em comentário |
|---|---|---|---|---|
| Fidelidade ao vocabulário do domínio brasileiro | alta — "competência" continua "competência" | baixa — força tradução artificial de termos sem equivalente | alta | média — o código é inglês, mas comentários preservam o termo |
| Legibilidade para quem só sabe Rust em inglês (contribuição internacional) | média (infraestrutura é legível; domínio exige glossário) | alta | baixa | alta no código, ainda exige glossário para entender a regra |
| Fricção na conversa com o cliente/contador durante desenvolvimento | baixa (mesmo vocabulário do código e da conversa) | alta (tradução mental constante) | nenhuma | alta (o código não fala a língua da conversa) |
| Coerência com convenções idiomáticas de Rust (`impl Display`, `Iterator`) | alta — infraestrutura segue o idioma nativo do ecossistema | alta | baixa (lutar contra convenções do próprio `std`) | alta |
| Ambiguidade de onde termina um domínio e começa infraestrutura | existe, mas é uma regra objetiva e verificável | não existe (tudo é uma coisa só) | não existe (tudo é uma coisa só) | existe, e é mais difícil de verificar automaticamente (depende de comentário, não de identificador) |
| Precedente no próprio ecossistema Rust | comum em domínios verticais nacionais | padrão do ecossistema geral | raro | raro |

Descartamos "tudo em inglês" porque o custo de tradução recorrente entre a conversa real com o cliente
e o código não se paga uma vez — se paga a cada revisão, a cada especificação de módulo, para sempre.
Descartamos "tudo em português" porque brigaria desnecessariamente com convenções inglesas inevitáveis
do próprio Rust (`impl Display for Dinheiro`) e tornaria a leitura de qualquer biblioteca de terceiro
uma mistura pior do que a fronteira deliberada que escolhemos.

## Decisão

**O vocabulário do domínio de negócio é escrito em português; a infraestrutura técnica (traits de
biblioteca padrão, padrões idiomáticos exigidos pelo ecossistema Rust) permanece em inglês.**

A regra prática, detalhada no [doc 15 §1](../15-convencoes-codigo.md): tipos, campos e funções que
expressam regra de negócio usam português sem acento em identificadores (`competencia`, `sangria`,
`baixar_parcela`) — com acento normal em toda string visível ao usuário e em toda documentação.
Traits de biblioteca padrão, tipos genéricos (`T`, `E`) e nomes exigidos pelo ecossistema permanecem
em inglês (`impl Display for Dinheiro`, `impl Iterator for Partidas`). `Resultado<T>` é o alias de
projeto para `Result<T, Erro>` e é usado sempre, em vez de misturar os dois. O
[glossário (doc 18)](../18-glossario.md) é o dicionário de referência: todo termo que aparece no
código aparece lá.

## Consequências

### Positivas

- Zero fricção de tradução entre a conversa real com o cliente e o código-fonte que implementa a
  regra discutida — "competência" no README do módulo é a mesma palavra no `struct Lancamento`.
- A especificação funcional de um módulo (README escrito antes do código,
  [doc 15 §2](../15-convencoes-codigo.md)) e o código resultante compartilham vocabulário sem camada
  de tradução — reduz erro de interpretação entre quem especifica e quem implementa.
- Nomes de domínio carregam nuance jurídica e contábil que uma tradução para o inglês perderia ou
  distorceria (por exemplo, "competência" não é exatamente `accrual`, tem peso específico no direito
  contábil brasileiro).
- Onboarding mais rápido para desenvolvedores brasileiros: não há um segundo vocabulário técnico a
  memorizar além do que já conhecem do próprio mercado em que atuam.

### Negativas

- Mistura visível de português e inglês no mesmo arquivo (`pub fn baixar_parcela(&self, parcela: Id,
  valor: Dinheiro) -> Resultado<Baixa>` mistura verbo de domínio em português com `Result`/`Id`
  conceitualmente ingleses) — para quem nunca viu a convenção, o primeiro contato causa estranheza.
- Contribuição internacional (um desenvolvedor de fora do Brasil, ou uma biblioteca de código aberto
  eventualmente extraída do projeto) fica mais difícil: o vocabulário de domínio não é autoexplicativo
  sem o glossário.
- Identificadores sem acento (`competencia`, não `competência`) divergem da grafia correta do
  português usada no texto visível e na documentação — alguém buscando por "competência" com acento
  no código-fonte não encontra nada.
- A fronteira "domínio vs. infraestrutura" exige julgamento em casos de borda (é `Origem` domínio ou
  infraestrutura? é claramente domínio) — a regra é objetiva na maioria dos casos, mas não é mecânica
  o suficiente para nunca gerar dúvida em revisão.

### Mitigações

- A regra é simples e verificável na prática: se o termo aparece em conversa com o cliente e no
  [glossário (doc 18)](../18-glossario.md), é domínio e vai em português; se é exigido pelo ecossistema
  Rust ou é genérico o bastante para não ter significado de negócio, é infraestrutura e vai em inglês
  ([doc 15 §1](../15-convencoes-codigo.md)).
- O glossário é mantido como parte obrigatória da documentação — todo termo novo que aparece no código
  precisa aparecer lá, e PRs que introduzem vocabulário de domínio sem atualizar o glossário são
  incompletos por definição de revisão ([doc 16](../16-onboarding.md)).
- Ausência de acento em identificador é convenção mecânica e previsível (remover acento é uma
  transformação determinística), não uma escolha arbitrária por arquivo — reduz a superfície real de
  confusão a uma regra memorizável em cinco minutos.
- Contribuição internacional não é um cenário priorizado hoje: o produto tem escopo nacional
  deliberado ([doc 00 §5](../00-visao-produto.md)), então o custo de fricção para colaboradores de fora
  do Brasil é aceito conscientemente, não ignorado por descuido.

## Quando revisitar

- Se o produto expandir escopo para atender mercados fora do Brasil — o que hoje é escopo
  explicitamente negativo ([doc 00 §5](../00-visao-produto.md), [doc 17](../17-roadmap.md)) — a
  decisão de idioma do domínio precisaria ser reaberta por completo, provavelmente para um modelo de
  chaves de tradução em vez de identificadores fixos em português.
- Se a contratação de desenvolvedores remotos fora do Brasil se tornar uma estratégia relevante de
  crescimento da equipe e a fricção de onboarding em código de domínio se mostrar, na prática, maior
  que a economia de tradução que a decisão hoje entrega.
- Se surgir um caso de uso concreto de extrair um crate do núcleo (por exemplo, `cardeal-kernel`) como
  biblioteca de código aberto de uso geral fora do domínio de ERP brasileiro — nesse caso, esse crate
  específico teria de ser revisto, não necessariamente o projeto inteiro.
- Não revisitaríamos por "mistura de idiomas incomoda quem chega de fora" isoladamente — esse
  desconforto inicial é esperado, documentado e resolvido em minutos pelo glossário; o ganho de
  fidelidade ao vocabulário do domínio para o público real do produto (equipe e clientes brasileiros)
  compensa esse custo pontual de adaptação.
