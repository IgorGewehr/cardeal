# ADR-0014 — Modularidade resolvida em runtime por manifesto declarativo

- **Status:** Aceita
- **Data:** 2026-09-01
- **Decisores:** Equipe de arquitetura
- **Relacionadas:** ADR-0001, ADR-0004

## Contexto

O Cardeal atende de um MEI que vende bolo a um posto com doze bicos de combustível, com o mesmo
produto ([doc 00 §3](../00-visao-produto.md)). O princípio de produto é explícito: "o usuário não vê o
que não usa" — menu, campos, relatórios e até colunas de grade são derivados do conjunto de módulos
ativos, sem campos cinzas "para quando você contratar" ([doc 00 §4](../00-visao-produto.md)). Isso é
uma exigência de UX antes de ser uma exigência técnica: o dono da padaria nunca deve ver a palavra
"encerrante", e o gerente do posto nunca deve ver "mapa de quartos"
([doc 04](../04-pilar-modularidade.md)).

A pergunta arquitetural é quando essa decisão de "o que aparece" é tomada. Builds separados por
vertical (um executável só para posto, outro só para hotel) resolveriam o problema de superfície
visível, mas multiplicariam o esforço de build, teste e distribuição por perfil — e, pior, tornariam
impossível o caso real e comum de um cliente que mistura verticais (um posto com loja de conveniência
e lava-jato usa `combustível + PDV + estoque + ordem de serviço` ao mesmo tempo,
[doc 00 §3](../00-visao-produto.md)). Plugins dinâmicos (`.dll`/`.so` carregados em runtime)
resolveriam a flexibilidade, mas comprometeriam a segurança de tipos entre o núcleo e o módulo — a
fronteira de ABI entre binários compilados separadamente é notoriamente frágil em Rust, onde o layout
de tipo não é estável entre versões do compilador. Microsserviços por módulo resolveriam isolamento,
mas contrariam diretamente a filosofia de instalação zero e o orçamento de RAM do
[doc 02](../02-pilar-eficiencia.md) — cada módulo microsserviço seria, no mínimo, um processo a mais
rodando.

O sistema já converge, por outro caminho, para uma resposta: cada módulo é um crate que implementa o
trait `Modulo` e se declara por um `Manifesto` lido no boot ([doc 04 §2](../04-pilar-modularidade.md)).
Essa infraestrutura de registro — usada, de qualquer forma, para orquestrar migrações, permissões e
menu — já é o mecanismo natural para decidir, em runtime, quais módulos aparecem para qual empresa.

## Alternativas consideradas

| Critério | **Manifesto em runtime, um binário** | Builds separados por vertical | Plugins dinâmicos (.dll/.so) | Microsserviços por módulo | Apenas feature flags de compilação |
|---|---|---|---|---|---|
| Um cliente pode misturar módulos de perfis diferentes (posto + loja + OS) | sim, trivialmente | não sem recompilar/reinstalar | sim, mas frágil | sim, mas com custo de infraestrutura | não (decidido antes de compilar) |
| Segurança de tipo entre núcleo e módulo | forte (compilado junto, mesmo `rustc`) | forte | frágil (ABI entre binários separados) | forte (mas via rede, com serialização) | forte |
| RAM adicional por módulo desligado | quase zero (código presente, mas inativo) | não aplicável (nem compilado) | zero se não carregado | um processo a mais rodando por módulo | não aplicável |
| Esforço de build e distribuição | um binário para todos os perfis | um binário por perfil, multiplicando CI e QA | um binário núcleo + N binários de plugin | um binário/processo por módulo | um binário por combinação de features |
| Desligar um módulo é seguro e reversível (dados intactos) | sim, por design | sim, mas exige reinstalar outro binário | depende da implementação do plugin | sim, mas com custo de coordenação distribuída | não aplicável (decidido em build) |
| Ativação por tenant sem reinstalar nada | sim | não | parcial | sim | não |

Plugins dinâmicos foram a alternativa mais seriamente avaliada, porque prometem exatamente a
flexibilidade de runtime sem exigir todo o código presente no binário. Foram descartados porque Rust
não garante estabilidade de ABI entre compilações separadas do mesmo tipo — carregar um `.dll`
compilado com uma versão de `rustc` ligeiramente diferente do núcleo é uma fonte real de undefined
behavior, e contornar isso exigiria uma camada de FFI estável (efetivamente, C) em toda fronteira de
módulo, destruindo boa parte do valor de ergonomia de ter Rust em todo o sistema
([ADR-0001](0001-rust-como-linguagem-unica.md)).

## Decisão

**O padrão de distribuição é um único binário com todos os módulos compilados; a modularidade real
acontece em runtime, por um manifesto declarativo por módulo e um perfil de empresa que ativa e
desativa módulos e submódulos por tenant.**

Os três níveis de modularidade estão descritos no [doc 04 §1](../04-pilar-modularidade.md):
compilação (features de Cargo, usadas apenas para casos especiais como um terminal PDV embarcado),
instância (manifesto de licença assinado — o que o cliente contratou) e tenant (perfil e chaves
ligadas/desligadas em runtime, decidido pelo admin). Cada módulo declara sua identidade e capacidades
por um `Manifesto` estático (permissões, menu, submódulos, contas requeridas, eventos publicados e
assinados — [doc 04 §2](../04-pilar-modularidade.md)), lido no boot em ordem topológica de
dependência. A UI inteira — sidebar, campos de formulário, colunas de grade — é **derivada** desse
registro, nunca escrita à mão por perfil ([doc 04 §4](../04-pilar-modularidade.md)): um módulo
desligado não aparece porque não existe entrada correspondente no registro consultado, não porque
uma condicional o esconde.

## Consequências

### Positivas

- Um mesmo binário atende todos os perfis de empresa do [doc 00 §3](../00-visao-produto.md) — não há
  matriz de builds a manter, testar e distribuir por vertical.
- Um cliente pode combinar módulos de perfis diferentes livremente (posto com PDV, loja de
  conveniência e ordem de serviço ao mesmo tempo) sem qualquer trabalho de engenharia adicional — é
  apenas configuração de tenant.
- Desligar um módulo é seguro e reversível por design: dados históricos permanecem no razão e nos
  relatórios, apenas a capacidade de criar registros novos e as telas correspondentes desaparecem
  ([doc 04 §6](../04-pilar-modularidade.md)) — o que dá ao cliente liberdade real de experimentar
  módulos sem medo de perder dado ao desligar.
- A UI deriva do mesmo registro usado para permissões e migrações — um único lugar de verdade sobre
  "o que existe", sem risco de a sidebar e o sistema de permissões divergirem sobre o que está ativo.
- Segurança de tipo plena entre núcleo e módulos: como tudo é compilado junto pelo mesmo `rustc`, não
  existe fronteira de ABI frágil a proteger.

### Negativas

- O binário final é maior do que seria um binário especializado por vertical: todo o código de todos
  os módulos está presente, ativo ou não — o alvo de binário do desktop
  (< 18 MB, [doc 02 §2.7](../02-pilar-eficiencia.md)) já contabiliza esse custo, mas ele cresce a cada
  módulo novo adicionado ao sistema.
- Toda a superfície de código de todos os módulos está presente no binário do cliente mesmo quando
  desligada — um módulo com uma vulnerabilidade, mesmo inativo para aquele tenant, ainda existe
  fisicamente no processo em execução, ampliando a superfície de ataque teórica.
- Verificações de "este módulo está desligado, então isso não deveria aparecer" ficam espalhadas por
  toda parte que renderiza UI condicional ou registra comportamento — cada tela, cada formulário,
  precisa lembrar de consultar o registro corretamente.
- Compilar o binário completo exige compilar todo o código de todos os módulos sempre, mesmo durante
  desenvolvimento local de um módulo específico — o que pesa no tempo de build incremental já discutido
  em [ADR-0001](0001-rust-como-linguagem-unica.md).

### Mitigações

- Features de Cargo continuam disponíveis para casos especiais de compilação (por exemplo, um
  terminal PDV embarcado dedicado, 40% menor sem os módulos de retaguarda,
  [doc 04 §1](../04-pilar-modularidade.md)) — a via de compilação não é eliminada, apenas não é o
  caminho padrão de distribuição.
- `cargo xtask verificar-perfis` sobe cada perfil de empresa em CI e captura a árvore de menu e os
  formulários resultantes, pegando automaticamente qualquer módulo que "vaze" em uma configuração onde
  deveria estar oculto ([doc 04 §8](../04-pilar-modularidade.md), [doc 14 §7](../14-testes-qualidade.md))
  — a verificação de vazamento de UI é um portão de CI, não uma esperança de disciplina de revisão.
- Uma macro central de declaração de comando obriga toda operação a declarar sua permissão
  (`#[comando(permissao = "...", ...)]`) — comando sem permissão não compila
  ([doc 08 §3.5](../08-seguranca-permissoes.md)) — o que centraliza a checagem de acesso em um único
  ponto verificado pelo compilador, em vez de espalhar a responsabilidade por cada tela.
- Regras invioláveis de módulo ([doc 04 §8](../04-pilar-modularidade.md)) — nunca depender de outro
  módulo no `Cargo.toml`, nunca ler tabela de outro módulo — mantêm o binário grande organizado em
  fronteiras claras, o que limita o dano potencial de uma vulnerabilidade em um módulo específico
  mesmo com todo o código presente no processo.

## Quando revisitar

- Se o tamanho do binário final ultrapassar consistentemente o orçamento do
  [doc 02 §2.7](../02-pilar-eficiencia.md) (< 18 MB desktop, < 9 MB servidor) mesmo depois de otimizar
  compilação (`lto = "fat"`, `strip = "symbols"`), a ponto de comprometer o tempo de download em
  conexões ruins do interior — cenário explicitamente relevante para o público do produto.
- Se surgir uma exigência real de isolamento de segurança entre módulos de tenants diferentes rodando
  no mesmo binário (por exemplo, um requisito de certificação que exija que código de um módulo
  desligado seja comprovadamente inacessível, não apenas inativo) — hoje não há esse requisito, mas se
  aparecer, mudaria o cálculo a favor de algum nível de isolamento de processo.
- Se `cargo xtask verificar-perfis` deixar de pegar vazamentos de UI de forma confiável (por exemplo,
  um vazamento relatado por cliente que a suíte de CI não capturou) mais de uma vez — sinal de que a
  cobertura de perfis testados precisa crescer ou que a estratégia de verificação automática precisa
  de reforço estrutural, não apenas mais casos de teste.
- Se o roadmap de "marketplace de módulos de terceiros" ([doc 17 — Fase 6](../17-roadmap.md)) avançar
  a ponto de exigir que um módulo de terceiro seja adicionado sem recompilar o binário principal —
  nesse ponto, a decisão contra plugins dinâmicos precisaria ser reaberta especificamente para esse
  caso de uso, com um mecanismo de sandboxing dedicado.
