# 02 — Pilar I: Eficiência de RAM e energia

> Um ERP de balcão roda 12 horas por dia, 6 dias por semana, em um PC que também tem que rodar o
> navegador do dono. Consumo não é vaidade técnica: é o que decide se o cliente compra um PC novo
> ou não, e se o nobreak aguenta 20 minutos ou 4.

## 1. Orçamento de recursos (contrato, não aspiração)

Esses números são **testados em CI** (`cargo xtask orcamento`) e o build falha se forem estourados.

| Cenário | RSS máx. | CPU em repouso | CPU em uso típico | Boot |
|---|---|---|---|---|
| Motor sozinho, base de 50k lançamentos | 40 MB | < 0,1 % | — | 300 ms |
| Motor, base de 5M lançamentos, 20 terminais | 180 MB | < 0,5 % | — | 800 ms |
| Terminal PDV (desktop + UI) | 90 MB | 0 % | < 8 % durante digitação | 400 ms |
| Monoposto (desktop + motor embutido) | 110 MB | < 0,1 % | — | 500 ms |
| Retaguarda com grade de 100k linhas aberta | 150 MB | 0 % | — | — |

Comparativo de referência (mesma carga, medido em bancada): ERP típico em Electron ≈ 480 MB;
ERP típico em .NET/WinForms com ORM ≈ 260 MB; ERP em Java/Swing ≈ 700 MB.

## 2. As sete decisões que produzem esses números

### 2.1 Zero repaint ocioso na UI

egui em modo reativo: a janela **não redesenha** se nada mudou. Sem timer de 60 fps, sem animação
perpétua. O loop fica bloqueado em `wait_events` e o processo consome **0% de CPU** com a tela aberta.

Animações existem (transição da sidebar, realce de linha) e são declaradas com prazo:
`ctx.request_repaint_after(Duration::from_millis(16))` só enquanto a animação vive. Terminada, o
processo volta a dormir.

**Efeito prático:** um PDV aberto o dia inteiro esperando o próximo cliente não gasta bateria de nobreak.

```rust
// cardeal-ui::app — o núcleo do loop reativo
impl eframe::App for Aplicativo {
    fn update(&mut self, ctx: &egui::Context, _f: &mut eframe::Frame) {
        // Nada aqui agenda repaint. Só interação do usuário, chegada de evento
        // do servidor ou animação viva acordam a janela.
        self.desenhar(ctx);
        if let Some(prazo) = self.animacoes.proximo_quadro() {
            ctx.request_repaint_after(prazo);
        }
    }
}
```

### 2.2 Nada de ORM, nada de reflexão em runtime

Consultas são SQL escrito à mão, preparadas uma vez e **cacheadas por conexão**
(`rusqlite::CachedStatement`). Não montamos árvore de expressão, não geramos SQL em runtime,
não materializamos entidades intermediárias.

Uma consulta de grade devolve linhas direto no struct de projeção, sem passar por um "modelo de domínio"
que seria descartado no frame seguinte.

### 2.3 Paginação por cursor, sempre

Nenhuma tela carrega uma tabela inteira. A grade virtualizada (`cardeal-ui::Grade`) pede exatamente as
linhas visíveis mais um buffer de 20, usando cursor keyset (`WHERE (data, id) < (?, ?) ORDER BY ... LIMIT n`)
— nunca `OFFSET`, que degrada linearmente.

Uma grade de 2 milhões de linhas usa a mesma memória de uma de 200: **~60 linhas materializadas**.

### 2.4 Um escritor, muitas leituras, um fsync

Ver [doc 07](07-persistencia-sqlite.md). Resumo do impacto energético: escritas concorrentes de vários
terminais são agrupadas numa janela de 2 ms e resolvidas em **um único fsync**. Vinte PDVs finalizando
vendas simultaneamente custam um write barrier no disco, não vinte.

Em SSD NVMe, um fsync custa ~0,3 ms; em HDD 5400 rpm de PDV antigo, ~12 ms. O group commit é a diferença
entre 80 vendas/s e 4 vendas/s naquele HDD.

### 2.5 Sem runtime async onde não há I/O concorrente

`cardeal-kernel` e `cardeal-ledger` são **síncronos e puros**. Async (`tokio`) existe só na borda:
servidor HTTP, cliente fiscal, drivers de rede. O código de domínio — que é 70% da base — não paga
o custo cognitivo nem o custo de alocação de futures.

No modo monoposto o runtime async sobe com `worker_threads = 2` (não `num_cpus`), porque não há
concorrência real a explorar.

### 2.6 Alocador e dependências enxutas

- `mimalloc` como alocador global no servidor (menor fragmentação em cargas de vida longa).
- Política de dependências: toda nova dependência direta exige justificativa no PR e passa por
  `cargo tree --duplicates`. Meta: **< 220 crates** no grafo total do workspace.
- `smallvec` para coleções que quase sempre têm poucos elementos (itens de um lançamento, partidas).
- `Arc<str>` em vez de `String` para dados de catálogo repetidos (nome de conta, descrição de produto
  em cache).
- Strings de erro estáticas onde possível; nada de `format!` em caminho quente.

### 2.7 Build otimizado para tamanho e velocidade

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = "symbols"

[profile.release.package."*"]
opt-level = 3

# Perfil para terminal PDV embarcado, onde tamanho importa mais que pico de throughput
[profile.balcao]
inherits = "release"
opt-level = "s"
```

Binário alvo: **< 18 MB** para o desktop (com fontes embutidas), **< 9 MB** para o servidor.

## 3. Consumo de energia

Energia é aproximadamente `CPU ativa × tempo + I/O`. As alavancas, em ordem de impacto:

| Alavanca | Ganho medido |
|---|---|
| Zero repaint ocioso | ~2,1 W → ~0,1 W num terminal ocioso |
| Group commit | reduz em ~85% os wakeups de disco em pico de vendas |
| Agendador com relógio único | um timer para todas as tarefas, não um por tarefa |
| Sem polling | tempo real por WebSocket (push), nunca `setInterval` perguntando ao servidor |
| `wgpu` com `PresentMode::Fifo` e sem vsync forçado | GPU não trabalha se não há quadro novo |

**Regra de ouro do projeto:** *nenhum componente pode acordar sozinho sem um motivo externo.*
Todo `sleep`/`interval` em loop no código precisa de comentário justificando por que não pôde ser
um evento. Isso é verificado em revisão.

### 3.1 Consciência de nobreak

No Windows, o motor consulta `GetSystemPowerStatus`. Ao detectar **operação em bateria**:

1. Emite evento `EnergiaEmBateria { minutos_estimados }`.
2. Escalona a durabilidade para o máximo (checkpoint de WAL imediato, `synchronous=FULL` já é o padrão
   no escritor).
3. Suspende tarefas não essenciais: exportação analítica, reindexação, backup completo.
4. A UI mostra faixa discreta no topo: *"Em bateria — 18 min. Vendas seguem normalmente."*
5. Abaixo de 5 minutos estimados, faz snapshot de segurança e sugere encerramento ordenado.

## 4. Como medimos

```bash
cargo xtask orcamento            # roda a suíte e compara com os tetos do contrato
cargo xtask orcamento --gravar   # atualiza a linha de base (exige aprovação em PR)
cargo bench -p cardeal-ledger    # criterion, com detecção de regressão
cargo xtask flamegraph -p cardeal-server --cenario pico-varejo
```

A suíte de orçamento sobe o processo de verdade, aplica um **cenário sintético realista**
(`testkit::cenarios::supermercado_bairro`: 3 PDVs, 1.200 cupons/dia, 8.000 SKUs, 90 dias de histórico)
e amostra RSS/CPU a cada 250 ms.

O resultado vira um artefato em `docs/medicoes/` versionado, para que regressões tenham histórico.

## 5. Anti-padrões proibidos

| Proibido | Por quê | Faça isso |
|---|---|---|
| `SELECT *` | Traz colunas que ninguém usa e quebra ao evoluir o esquema | Liste as colunas |
| `Vec<Entidade>` de tabela inteira em memória | Cresce sem teto com o cliente | Cursor + grade virtualizada |
| Timer de polling | Acorda a CPU sem motivo | Evento push |
| `clone()` de coleção grande para "facilitar" | Alocação invisível em caminho quente | Empreste, ou use `Arc` |
| `format!` em loop de renderização | Aloca 60x por segundo | Buffer reutilizável, `write!` |
| Nova dependência "porque é conveniente" | Cada crate é build time, binário e superfície de risco | Justifique no PR |
| `unwrap()` fora de teste | Derruba o processo do cliente | `?` com erro de domínio |

## 6. Onde deliberadamente gastamos

Eficiência não é avareza. Gastamos sem culpa em:

- **Índices.** Disco é barato; varredura de tabela em hora de pico não é.
- **Cache de catálogo em memória** no terminal PDV (produtos, preços, clientes frequentes): ~8 MB que
  eliminam ida ao servidor a cada item bipado e permitem o modo autônomo.
- **Fontes embutidas no binário** (~4 MB): garantem identidade visual idêntica em qualquer Windows.
- **Testes.** A suíte pode demorar; o cliente não pode.
