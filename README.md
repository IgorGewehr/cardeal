<div align="center">

<img src="assets/marca/cardeal-icone-256.png" alt="Cardeal" width="96" height="96">

# Cardeal ERP

**Um ERP brasileiro de código aberto escrito em Rust, com o financeiro no centro.**

`eficiência` · `resiliência` · `modularidade`

</div>

---

## O que é

Cardeal é um ERP para o mercado brasileiro construído sobre três pilares inegociáveis:

| Pilar | Compromisso |
|---|---|
| **Eficiência** | Servidor idle < 40 MB RSS, terminal PDV < 90 MB, 0% de CPU em repouso. Roda em hardware de balcão de 2015. |
| **Resiliência** | Sobrevive a queda de energia no meio de uma venda e a queda de internet por dias. Nunca perde um centavo já registrado. |
| **Modularidade** | Um único executável atende MEI, posto de combustível, micro indústria e hotel. O usuário só enxerga o que lhe interessa. |

E uma tese central de arquitetura:

> **O financeiro não é um módulo. É o barramento.**
> Toda venda, aluguel, ordem de serviço, diária de hotel ou abastecimento vira, no mesmo instante e na mesma transação,
> um lançamento de partidas dobradas no Razão. Não existe "integrar com o financeiro" — tudo já nasce dentro dele.

Consequência prática: o dado de gestão é subproduto natural da operação, não um relatório montado depois.

## Estado do projeto

Em desenvolvimento ativo. **O que existe de verdade agora, testado e funcionando** (não a
visão final) está em [`docs/19-estado-e-processo.md`](docs/19-estado-e-processo.md) — leia
esse antes do roadmap se for continuar o trabalho. Resumo: `cardeal-kernel`, `cardeal-ledger`
(domínio), `cardeal-modkit` (manifesto) e `cardeal-auth` (domínio) implementados e testados
(95 testes + 11 doctests, 100% verde); os demais 24 crates são stub. Visão de longo prazo em
[`docs/17-roadmap.md`](docs/17-roadmap.md).

## Documentação

Comece por [`docs/README.md`](docs/README.md) — é o índice mestre.

Atalhos:

- **Estou retomando o projeto**: [`docs/19-estado-e-processo.md`](docs/19-estado-e-processo.md) (leia primeiro)
- Nunca vi o projeto: [`docs/16-onboarding.md`](docs/16-onboarding.md) (o "primeiro dia")
- Quero entender a arquitetura: [`docs/01-arquitetura-geral.md`](docs/01-arquitetura-geral.md)
- Quero entender o financeiro: [`docs/05-nucleo-financeiro.md`](docs/05-nucleo-financeiro.md)
- Quero escrever um módulo: [`docs/04-pilar-modularidade.md`](docs/04-pilar-modularidade.md)
- Quero entender as telas: [`docs/12-ui-ux.md`](docs/12-ui-ux.md)
- Não entendi um termo: [`docs/18-glossario.md`](docs/18-glossario.md)

## Compilando

```bash
# 1. Instale a toolchain (uma vez)
winget install Rustlang.Rustup        # Windows
# ou: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Compile o workspace
cargo build --workspace

# 3. Suba o motor e o terminal
cargo run -p cardeal-server
cargo run -p cardeal-desktop
```

Tarefas auxiliares vivem em `cargo xtask --help` (migrações, seed, benchmarks, teste de queda de energia).

## Licença

A definir.
