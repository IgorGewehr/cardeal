# 14 — Testes e qualidade

## 1. A pirâmide

```
                    ╱╲          Cenários end-to-end (12)
                   ╱  ╲         "um dia de operação de um posto"
                  ╱────╲
                 ╱      ╲       Integração de módulo (~200)
                ╱        ╲      comando → banco → razão → evento
               ╱──────────╲
              ╱            ╲    Propriedade (~60)
             ╱              ╲   invariantes que valem para toda entrada
            ╱────────────────╲
           ╱                  ╲ Unidade (~1500)
          ╱____________________╲ domínio puro, sem I/O
```

Metas de cobertura: **90%** em `cardeal-kernel` e `cardeal-ledger` (são o alicerce),
**80%** nos módulos, **sem meta** em UI (testada por captura de tela e testes de fluxo).

## 2. Unidade — domínio puro

A pasta `dominio/` de cada módulo **não tem I/O**. Isso é uma regra arquitetural, não um estilo:
torna o teste instantâneo e sem fixture.

```rust
#[test]
fn desconto_nao_pode_superar_limite_do_papel() {
    let item = ItemVenda::novo(produto(), qtd(2), preco("10,00"));
    let erro = item.aplicar_desconto(pct("30"), LimiteDesconto::pct("20")).unwrap_err();
    assert_eq!(erro.codigo(), CodigoErro::LimiteExcedido);
}
```

## 3. Propriedade — os invariantes

`proptest`. É aqui que mora a garantia real do núcleo financeiro.

```rust
proptest! {
    /// Nenhuma sequência de operações pode produzir lançamento desbalanceado.
    #[test]
    fn razao_sempre_balanceado(ops in vec(any::<OperacaoNegocio>(), 1..200)) {
        let mut mundo = Mundo::novo();
        for op in ops { let _ = mundo.aplicar(op); }   // erros são aceitáveis; corrupção não
        prop_assert_eq!(mundo.soma_de_todas_as_partidas(), Dinheiro::ZERO);
    }

    /// Ratear um valor em N parcelas nunca perde nem cria centavos.
    #[test]
    fn rateio_conserva_o_total(total in 1i64..100_000_000, n in 1usize..360) {
        let partes = Dinheiro::centavos(total).ratear(n);
        prop_assert_eq!(partes.iter().copied().sum::<Dinheiro>(), Dinheiro::centavos(total));
        prop_assert!(partes.iter().max().unwrap().centavos()
                   - partes.iter().min().unwrap().centavos() <= 1);
    }

    /// Estornar tudo devolve todos os saldos a zero.
    #[test]
    fn estorno_e_inverso(ops in vec(any::<OperacaoNegocio>(), 1..80)) {
        let mut mundo = Mundo::novo();
        let ids = ops.into_iter().filter_map(|o| mundo.aplicar(o).ok()).collect::<Vec<_>>();
        for id in ids.into_iter().rev() { mundo.estornar(id).unwrap(); }
        prop_assert!(mundo.saldos_todos_zerados());
    }

    /// Sincronizar uma fila offline em qualquer ordem de retentativa dá o mesmo resultado.
    #[test]
    fn reconciliacao_e_idempotente(fila in fila_offline(), embaralhar in any::<u64>()) {
        let a = reconciliar(fila.clone(), Ordem::Original);
        let b = reconciliar(fila,          Ordem::ComRetentativas(embaralhar));
        prop_assert_eq!(a.estado_final(), b.estado_final());
    }
}
```

## 4. Integração de módulo

Banco SQLite em memória, motor real, relógio controlado.

```rust
#[test]
fn venda_no_pdv_lanca_no_razao_e_baixa_estoque() {
    let mut m = Motor::teste()
        .com_modulos(&["financeiro", "estoque", "vendas", "pdv"])
        .com_produto("7891000053508", "Leite", custo("3,20"), preco("5,49"))
        .com_estoque("7891000053508", qtd(100))
        .com_caixa_aberto("Caixa 1", saldo("200,00"))
        .construir();

    let r = m.comando(FinalizarVenda {
        itens: vec![item("7891000053508", qtd(2))],
        pagamentos: vec![dinheiro("10,98")],
        ..Default::default()
    }).unwrap();

    // razão
    assert_lancamento!(m, r.lancamento, {
        debito("1.1.01 Caixa")            => "10,98",
        credito("4.1 Receita de vendas")  => "10,98",
        debito("5.1 CMV")                 =>  "6,40",
        credito("1.3.01 Estoque")         =>  "6,40",
    });
    // estoque
    assert_eq!(m.saldo("7891000053508"), qtd(98));
    // caixa
    assert_eq!(m.saldo_conta("1.1.01"), dinheiro("210,98"));
    // evento
    assert_evento!(m, "pdv.venda_finalizada.v1", { total: "10,98" });
    // invariante global
    m.provar_razao().unwrap();
}
```

A macro `assert_lancamento!` é o que mantém o **receituário** do
[doc 05 §5](05-nucleo-financeiro.md#5-o-receituário-como-cada-módulo-posta) honesto: cada linha da
tabela tem um teste correspondente.

## 5. Teste de queda de energia

Descrito em [doc 03 §7](03-pilar-resiliencia.md). Roda em CI semanalmente e a cada release:
500 ciclos de carga + `SIGKILL` + verificação completa. É o teste que dá o direito de prometer o
Pilar II.

## 6. Cenários end-to-end

Cada cenário é um **dia inteiro** de um tipo de empresa, escrito em um DSL declarativo:

```rust
cenario!("posto: um dia de operação", perfil = "posto", {
    07:00 => abrir_turno("Pista", operador = "Carlos");
    07:00 => aferir_bombas();
    07:12 => abastecimento(bico = 3, litros = 42.7, pagamento = Dinheiro);
    07:31 => abastecimento(bico = 1, litros = 30.0, pagamento = CartaoDebito);
    08:00 => venda_conveniencia(itens = 3, pagamento = Pix);
    10:00 => desconecta_rede();                   // servidor cai
    10:05 => abastecimento(bico = 2, litros = 55.1, pagamento = Dinheiro);  // modo autônomo
    10:40 => reconecta_rede();
    12:00 => entrada_combustivel(produto = "Gasolina Comum", litros = 15_000);
    18:00 => aferir_tanques();
    22:00 => fechar_turno();
    verificar => {
        razao_balanceado();
        estoque_bate_com_encerrante(tolerancia = litros(2.0));
        caixa_bate_com_pagamentos();
        todas_vendas_offline_reconciliadas();
        lmc_do_dia_gerado();
    }
});
```

Cenários existentes: MEI, padaria, mercado de bairro, posto, hotel, locadora, oficina, micro
indústria, distribuidora, restaurante, farmácia, loja de roupas.

Além de testar, eles **são a documentação executável** do produto e a base para a demonstração
comercial — a mesma fixture que testa também popula o modo demonstração.

## 7. Testes de UI

- **Captura de tela:** cada tela é renderizada fora da tela (`egui` headless) e comparada com uma
  imagem de referência. Diferença acima de 0,3% falha e o CI publica o diff visual.
- **Perfis:** `xtask verificar-perfis` sobe cada perfil e serializa a árvore de menu e os campos de
  cada formulário. Um módulo que "vaza" numa configuração onde deveria estar oculto é pego aqui.
- **Contraste:** todo par (texto, fundo) do design system é verificado contra WCAG AA.
- **Fluxo de teclado:** um teste simula a sequência real do operador
  (`F3 leite Enter 2 F2 dinheiro Enter`) e valida a venda resultante.

## 8. Desempenho

```bash
cargo bench -p cardeal-ledger        # criterion, falha em regressão > 10%
cargo xtask orcamento                # RAM/CPU/boot contra o contrato do doc 02
cargo xtask carga --cenario mercado  # 8 h de operação em 4 min, mede p99
```

Resultados versionados em `docs/medicoes/`, com gráfico de evolução por commit.

## 9. Fuzzing

`cargo fuzz` em tudo que lê entrada externa: parser de XML de NFe, OFX, CNAB, código de barras,
protocolo binário, importador de CSV. Roda continuamente em CI noturno; corpus versionado.

## 10. Portões de CI

Nenhum merge passa sem:

| Portão | Comando |
|---|---|
| Formatação | `cargo fmt --check` |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings` |
| Testes | `cargo test --workspace` |
| Regra de dependência | `cargo xtask arquitetura` (verifica os anéis do doc 01) |
| Dependências | `cargo deny check` + `cargo audit` |
| Cobertura | `cargo llvm-cov` com mínimos por crate |
| Orçamento de recursos | `cargo xtask orcamento` |
| Contraste | `cargo xtask contraste` |
| Perfis | `cargo xtask verificar-perfis` |
| Documentação | `cargo doc --no-deps -D warnings` + links quebrados nos `.md` |
| Compatibilidade de protocolo | testes de ouro em `testes/ouro/` |

Semanal: queda de energia (500 ciclos), fuzzing (4 h), carga (2 h), MSRV.

## 11. Regras de revisão

1. Todo PR referencia a seção da documentação que ele implementa ou altera.
2. Mudança de comportamento financeiro exige teste no receituário.
3. Migração exige teste de migração.
4. Comando novo exige permissão declarada e teste de autorização negada.
5. Consulta nova exige `EXPLAIN QUERY PLAN` no PR mostrando uso de índice.
6. Dependência nova exige justificativa e o resultado de `cargo tree --duplicates`.
7. `unwrap`, `expect` e `panic!` fora de teste exigem comentário `// SEGURO:` justificando.
