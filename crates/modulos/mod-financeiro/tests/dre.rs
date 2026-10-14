//! A DRE gerencial lida do Razão real, pelo `Despachante`.

#![allow(clippy::result_large_err)]

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel, Sessao};
use cardeal_kernel::{Data, Dinheiro, Fuso, Id, Instante, Periodo};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_financeiro::dre::Dre;
use mod_financeiro::{
    BaixarPagamento, BaixarRecebimento, ComparativoDre, DfcDoPeriodo, DreComparativa, DreDoPeriodo,
    LancarTituloAPagar, LancarTituloAReceber, MeioPagamento, ModuloFinanceiro, MANIFESTO,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tempfile::TempDir;

struct Cena {
    _dir: TempDir,
    arm: Armazenamento,
    empresa: Id,
    d: Despachante,
}

fn cena() -> Cena {
    let dir = tempfile::tempdir().unwrap();
    let arm =
        Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("cardeal.db"))).unwrap();
    arm.migrar(&[
        cardeal_ledger::migracoes::conjunto(),
        ModuloFinanceiro.migracoes(),
    ])
    .unwrap();
    let empresa = Id::novo();
    arm.escritor()
        .executar(
            ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()),
            move |uow| {
                uow.conexao()
                    .execute(
                        "INSERT INTO nucleo_empresa
                           (id, razao_social, nome_fantasia, cnpj, regime, endereco, perfil, criado_em)
                         VALUES (?1,'Teste LTDA','Teste','11222333000181','SimplesNacional','{}','comercio',0)",
                        [empresa.em_bytes().as_slice()],
                    )
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                semear_plano_padrao(uow, empresa)
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            },
        )
        .unwrap();
    Cena {
        _dir: dir,
        arm,
        empresa,
        d: Despachante::construir(&[&ModuloFinanceiro]).unwrap(),
    }
}

impl Cena {
    fn sessao(&self, usuario: Id, permissoes: &[&str]) -> Sessao {
        let mut papel = Papel::novo(self.empresa, "Teste");
        for p in permissoes {
            papel = papel.com_permissao(*p);
        }
        Sessao::abrir(EmissaoSessao::padrao(
            usuario,
            Id::novo(),
            Escopo::empresa_inteira(self.empresa),
            AutorizacoesEfetivas::consolidar([&papel]),
            Instante::EPOCA,
        ))
    }

    fn ambiente(&self) -> Ambiente {
        let mut rm = RegistroModulos::novo();
        rm.registrar(&MANIFESTO).unwrap();
        let efetivo = rm
            .resolver(&PedidoAtivacao::nova().com_modulo("financeiro"))
            .unwrap();
        Ambiente::novo(self.empresa, efetivo)
    }

    fn cmd<T: DeserializeOwned>(&self, s: &Sessao, nome: &str, c: &impl Serialize) -> T {
        let saida = self
            .d
            .executar_comando(
                nome,
                &postcard::to_stdvec(c).unwrap(),
                s,
                &self.ambiente(),
                self.arm.escritor(),
            )
            .unwrap();
        postcard::from_bytes(&saida).unwrap()
    }

    fn consulta<T: DeserializeOwned>(&self, s: &Sessao, nome: &str, c: &impl Serialize) -> T {
        let saida = self
            .d
            .executar_consulta(
                nome,
                &postcard::to_stdvec(c).unwrap(),
                s,
                &self.ambiente(),
                self.arm.leitor(),
            )
            .unwrap();
        postcard::from_bytes(&saida).unwrap()
    }
}

fn hoje() -> Data {
    Data::hoje(Fuso::BRASILIA)
}

fn lancar_receita(c: &Cena, s: &Sessao, valor: Dinheiro, emissao: Data) {
    let _: mod_financeiro::TituloAReceberLancado = c.cmd(
        s,
        "financeiro.lancar_titulo_a_receber.v1",
        &LancarTituloAReceber {
            cliente: None,
            valor_total: valor,
            emissao,
            parcelas: 1,
            primeiro_vencimento: emissao,
            intervalo_dias: 0,
            observacao: None,
            categoria: None,
            quitado_agora: None,
        },
    );
}

fn lancar_despesa(c: &Cena, s: &Sessao, valor: Dinheiro, emissao: Data) {
    let _: mod_financeiro::TituloAPagarLancado = c.cmd(
        s,
        "financeiro.lancar_titulo_a_pagar.v1",
        &LancarTituloAPagar {
            fornecedor: None,
            valor_total: valor,
            emissao,
            parcelas: 1,
            primeiro_vencimento: emissao,
            intervalo_dias: 0,
            observacao: None,
            categoria: None,
            quitado_agora: None,
        },
    );
}

#[test]
fn dre_do_periodo_le_o_razao_e_fecha_o_lucro() {
    let c = cena();
    let s = c.sessao(
        Id::novo(),
        &[
            "financeiro.receber.criar",
            "financeiro.pagar.criar",
            "financeiro.dre.ver",
        ],
    );
    let h = hoje();
    lancar_receita(&c, &s, Dinheiro::reais(1_000), h);
    lancar_despesa(&c, &s, Dinheiro::reais(400), h);

    let dre: Dre = c.consulta(
        &s,
        "financeiro.dre_do_periodo.v1",
        &DreDoPeriodo {
            periodo: Periodo::novo(h.inicio_do_mes(), h.fim_do_mes()),
        },
    );
    assert_eq!(
        dre.valor_de("(+) Outras receitas"),
        Some(Dinheiro::reais(1_000))
    );
    assert_eq!(
        dre.valor_de("(−) Despesas operacionais"),
        Some(Dinheiro::reais(-400))
    );
    assert_eq!(dre.lucro_liquido, Dinheiro::reais(600));

    // Outro mês: nada.
    let vazio: Dre = c.consulta(
        &s,
        "financeiro.dre_do_periodo.v1",
        &DreDoPeriodo {
            periodo: Periodo::novo(
                h.mais_meses(-3).inicio_do_mes(),
                h.mais_meses(-3).fim_do_mes(),
            ),
        },
    );
    assert_eq!(vazio.lucro_liquido, Dinheiro::ZERO);
}

#[test]
fn comparativo_separa_mes_atual_anterior_e_ano_anterior() {
    let c = cena();
    let s = c.sessao(
        Id::novo(),
        &[
            "financeiro.receber.criar",
            "financeiro.pagar.criar",
            "financeiro.dre.ver",
        ],
    );
    let h = hoje();
    lancar_receita(&c, &s, Dinheiro::reais(900), h);
    lancar_receita(&c, &s, Dinheiro::reais(500), h.mais_meses(-1));
    lancar_despesa(&c, &s, Dinheiro::reais(100), h.mais_meses(-12));

    let cmp: ComparativoDre = c.consulta(
        &s,
        "financeiro.dre_comparativa.v1",
        &DreComparativa {
            mes: h.competencia(),
        },
    );
    assert_eq!(cmp.atual.lucro_liquido, Dinheiro::reais(900));
    assert_eq!(cmp.mes_anterior.lucro_liquido, Dinheiro::reais(500));
    assert_eq!(
        cmp.mesmo_mes_ano_anterior.lucro_liquido,
        Dinheiro::reais(-100)
    );
}

#[test]
fn sem_permissao_a_dre_e_recusada() {
    let c = cena();
    let s = c.sessao(Id::novo(), &["financeiro.receber.criar"]);
    let h = hoje();
    let r = c.d.executar_consulta(
        "financeiro.dre_do_periodo.v1",
        &postcard::to_stdvec(&DreDoPeriodo {
            periodo: Periodo::novo(h.inicio_do_mes(), h.fim_do_mes()),
        })
        .unwrap(),
        &s,
        &c.ambiente(),
        c.arm.leitor(),
    );
    assert!(r.is_err());
}

fn baixar_receber(c: &Cena, s: &Sessao, parcela: Id, valor: Dinheiro) {
    let _: mod_financeiro::RecebimentoBaixado = c.cmd(
        s,
        "financeiro.baixar_recebimento.v1",
        &BaixarRecebimento {
            parcela,
            valor,
            data: hoje(),
            meio_pagamento: MeioPagamento::Dinheiro,
            conta_destino: None,
        },
    );
}

#[test]
fn dfc_so_conta_o_que_foi_realizado_e_o_saldo_final_bate_com_o_caixa() {
    let c = cena();
    let s = c.sessao(
        Id::novo(),
        &[
            "financeiro.receber.criar",
            "financeiro.receber.baixar",
            "financeiro.pagar.criar",
            "financeiro.pagar.baixar",
            "financeiro.dre.ver",
        ],
    );
    let h = hoje();
    // Lançados: receber 1.000 e pagar 400. Só o recebimento de 600 e o pagamento de 150 saem do caixa.
    let r: mod_financeiro::TituloAReceberLancado = c.cmd(
        &s,
        "financeiro.lancar_titulo_a_receber.v1",
        &LancarTituloAReceber {
            cliente: None,
            valor_total: Dinheiro::reais(1_000),
            emissao: h,
            parcelas: 1,
            primeiro_vencimento: h,
            intervalo_dias: 0,
            observacao: None,
            categoria: None,
            quitado_agora: None,
        },
    );
    let p: mod_financeiro::TituloAPagarLancado = c.cmd(
        &s,
        "financeiro.lancar_titulo_a_pagar.v1",
        &LancarTituloAPagar {
            fornecedor: None,
            valor_total: Dinheiro::reais(400),
            emissao: h,
            parcelas: 1,
            primeiro_vencimento: h,
            intervalo_dias: 0,
            observacao: None,
            categoria: None,
            quitado_agora: None,
        },
    );
    baixar_receber(&c, &s, r.parcelas[0], Dinheiro::reais(600));
    let _: mod_financeiro::PagamentoBaixado = c.cmd(
        &s,
        "financeiro.baixar_pagamento.v1",
        &BaixarPagamento {
            parcela: p.parcelas[0],
            valor: Dinheiro::reais(150),
            data: h,
            meio_pagamento: MeioPagamento::Dinheiro,
            conta_destino: None,
        },
    );

    let dfc: mod_financeiro::dfc::Dfc = c.consulta(
        &s,
        "financeiro.dfc_do_periodo.v1",
        &DfcDoPeriodo {
            periodo: Periodo::novo(h.inicio_do_mes(), h.fim_do_mes()),
        },
    );
    let operacional = &dfc.blocos[0];
    let valor_de = |rotulo: &str| {
        operacional
            .linhas
            .iter()
            .find(|l| l.rotulo == rotulo)
            .map(|l| l.valor)
    };
    assert_eq!(
        valor_de("Recebimentos de clientes"),
        Some(Dinheiro::reais(600))
    );
    assert_eq!(
        valor_de("Pagamentos a fornecedores"),
        Some(Dinheiro::reais(-150))
    );
    assert_eq!(dfc.variacao, Dinheiro::reais(450));
    assert_eq!(dfc.saldo_inicial, Dinheiro::ZERO);

    // O fechamento é o saldo realizado das contas de caixa no Razão.
    let saldo_razao: i64 = c
        .arm
        .leitor()
        .consultar(|conn| {
            conn.query_row(
                "SELECT COALESCE(SUM(rp.valor),0) FROM razao_partida rp
                 JOIN razao_conta rc ON rc.id = rp.conta
                 JOIN razao_lancamento rl ON rl.id = rp.lancamento
                 WHERE rc.empresa = ?1 AND rc.papel_padrao IN ('Caixa','Bancos','ValoresEmTransito')
                   AND rl.estado IN ('Realizado','Estornado')",
                [c.empresa.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap();
    assert_eq!(dfc.saldo_final.em_centavos(), saldo_razao);

    // Mês seguinte: abre com o fechamento do anterior e não tem movimento.
    let seguinte: mod_financeiro::dfc::Dfc = c.consulta(
        &s,
        "financeiro.dfc_do_periodo.v1",
        &DfcDoPeriodo {
            periodo: Periodo::novo(
                h.fim_do_mes().mais_dias(1),
                h.fim_do_mes().mais_dias(1).fim_do_mes(),
            ),
        },
    );
    assert_eq!(seguinte.saldo_inicial, Dinheiro::reais(450));
    assert_eq!(seguinte.variacao, Dinheiro::ZERO);
}

#[test]
fn saude_financeira_sai_verde_com_folga_e_vermelha_no_aperto() {
    use mod_financeiro::saude::{Saude, Semaforo};
    use mod_financeiro::SaudeFinanceira;

    // Folga: a receber 1.000 e a pagar 400, ambos em 10 dias, sem atraso.
    let c = cena();
    let s = c.sessao(
        Id::novo(),
        &[
            "financeiro.receber.criar",
            "financeiro.pagar.criar",
            "financeiro.dre.ver",
        ],
    );
    let h = hoje();
    let em_10 = h.mais_dias(10);
    let receber = |valor: i64, venc: Data| {
        let _: mod_financeiro::TituloAReceberLancado = c.cmd(
            &s,
            "financeiro.lancar_titulo_a_receber.v1",
            &LancarTituloAReceber {
                cliente: None,
                valor_total: Dinheiro::reais(valor),
                emissao: h,
                parcelas: 1,
                primeiro_vencimento: venc,
                intervalo_dias: 0,
                observacao: None,
                categoria: None,
                quitado_agora: None,
            },
        );
    };
    let pagar = |valor: i64, venc: Data| {
        let _: mod_financeiro::TituloAPagarLancado = c.cmd(
            &s,
            "financeiro.lancar_titulo_a_pagar.v1",
            &LancarTituloAPagar {
                fornecedor: None,
                valor_total: Dinheiro::reais(valor),
                emissao: h,
                parcelas: 1,
                primeiro_vencimento: venc,
                intervalo_dias: 0,
                observacao: None,
                categoria: None,
                quitado_agora: None,
            },
        );
    };
    receber(1_000, em_10);
    pagar(400, em_10);
    let bom: Saude = c.consulta(
        &s,
        "financeiro.saude.v1",
        &SaudeFinanceira { giro_mensal: None },
    );
    assert_eq!(bom.semaforo, Semaforo::Verde, "{bom:?}");
    assert!(bom
        .sinais
        .iter()
        .any(|x| x.nome == "Giro de estoque" && x.pontos.is_none()));

    // Aperto: só contas vencidas a pagar e um recebível atrasado há mais de 30 dias.
    let c2 = cena();
    let s2 = c2.sessao(
        Id::novo(),
        &[
            "financeiro.receber.criar",
            "financeiro.pagar.criar",
            "financeiro.dre.ver",
        ],
    );
    let _: mod_financeiro::TituloAPagarLancado = c2.cmd(
        &s2,
        "financeiro.lancar_titulo_a_pagar.v1",
        &LancarTituloAPagar {
            fornecedor: None,
            valor_total: Dinheiro::reais(5_000),
            emissao: h.mais_dias(-20),
            parcelas: 1,
            primeiro_vencimento: h.mais_dias(-5),
            intervalo_dias: 0,
            observacao: None,
            categoria: None,
            quitado_agora: None,
        },
    );
    let _: mod_financeiro::TituloAReceberLancado = c2.cmd(
        &s2,
        "financeiro.lancar_titulo_a_receber.v1",
        &LancarTituloAReceber {
            cliente: None,
            valor_total: Dinheiro::reais(500),
            emissao: h.mais_dias(-60),
            parcelas: 1,
            primeiro_vencimento: h.mais_dias(-45),
            intervalo_dias: 0,
            observacao: None,
            categoria: None,
            quitado_agora: None,
        },
    );
    let ruim: Saude = c2.consulta(
        &s2,
        "financeiro.saude.v1",
        &SaudeFinanceira { giro_mensal: None },
    );
    assert_eq!(ruim.semaforo, Semaforo::Vermelho, "{ruim:?}");
    assert!(!ruim.puxando_para_baixo().is_empty());
}

/// Um lançamento direto no Razão, por código de conta: (código, valor) com débito positivo e
/// crédito negativo; `realizado` = o dinheiro andou na data.
fn lancar_no_razao(c: &Cena, historico: &str, partidas: &[(&str, i64)], realizado: bool) {
    use cardeal_ledger::{ConstrutorLancamento, Origem, Razao, RepositorioRazao};
    let empresa = c.empresa;
    let historico = historico.to_owned();
    let partidas: Vec<(String, i64)> = partidas
        .iter()
        .map(|(k, v)| ((*k).to_owned(), *v))
        .collect();
    c.arm
        .escritor()
        .executar(
            ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()),
            move |uow| {
                let erro = |e: String| ErroArmazenamento::Sqlite(e);
                let mut l = ConstrutorLancamento::novo(empresa, hoje(), historico)
                    .origem(Origem::modulo("teste").tipo("manual"))
                    .criado_por(Id::novo(), Id::novo())
                    .agora(Instante::EPOCA);
                if realizado {
                    l = l.liquidacao(hoje());
                }
                for (codigo, valor) in &partidas {
                    let conta: Vec<u8> = uow
                        .conexao()
                        .query_row(
                            "SELECT id FROM razao_conta WHERE empresa = ?1 AND codigo = ?2",
                            rusqlite::params![empresa.em_bytes().as_slice(), codigo],
                            |r| r.get(0),
                        )
                        .map_err(|e| erro(e.to_string()))?;
                    let conta = Id::de_bytes(conta.as_slice().try_into().unwrap());
                    l = if *valor > 0 {
                        l.debitar(conta, Dinheiro::reais(*valor))
                    } else {
                        l.creditar(conta, Dinheiro::reais(-*valor))
                    };
                }
                let l = l.construir().map_err(|e| erro(e.to_string()))?;
                Razao::registrar(&mut RepositorioRazao::novo(uow), l)
                    .map_err(|e| erro(e.to_string()))?;
                Ok(())
            },
        )
        .unwrap();
}

#[test]
fn dfc_indireto_vai_do_lucro_ao_mesmo_caixa_do_direto() {
    use mod_financeiro::dfc::{Dfc, DfcIndireto, LUCRO_LIQUIDO};
    use mod_financeiro::DfcIndiretoDoPeriodo;

    let c = cena();
    let s = c.sessao(
        Id::novo(),
        &[
            "financeiro.receber.criar",
            "financeiro.receber.baixar",
            "financeiro.pagar.criar",
            "financeiro.pagar.baixar",
            "financeiro.dre.ver",
        ],
    );
    let h = hoje();
    // Vendeu 1.000 a prazo e recebeu 600; comprou 400 a prazo e pagou 150.
    let r: mod_financeiro::TituloAReceberLancado = c.cmd(
        &s,
        "financeiro.lancar_titulo_a_receber.v1",
        &LancarTituloAReceber {
            cliente: None,
            valor_total: Dinheiro::reais(1_000),
            emissao: h,
            parcelas: 1,
            primeiro_vencimento: h,
            intervalo_dias: 0,
            observacao: None,
            categoria: None,
            quitado_agora: None,
        },
    );
    let p: mod_financeiro::TituloAPagarLancado = c.cmd(
        &s,
        "financeiro.lancar_titulo_a_pagar.v1",
        &LancarTituloAPagar {
            fornecedor: None,
            valor_total: Dinheiro::reais(400),
            emissao: h,
            parcelas: 1,
            primeiro_vencimento: h,
            intervalo_dias: 0,
            observacao: None,
            categoria: None,
            quitado_agora: None,
        },
    );
    baixar_receber(&c, &s, r.parcelas[0], Dinheiro::reais(600));
    let _: mod_financeiro::PagamentoBaixado = c.cmd(
        &s,
        "financeiro.baixar_pagamento.v1",
        &BaixarPagamento {
            parcela: p.parcelas[0],
            valor: Dinheiro::reais(150),
            data: h,
            meio_pagamento: MeioPagamento::Dinheiro,
            conta_destino: None,
        },
    );
    // Depreciação de 20 (não sai do caixa), máquina de 500 à vista, empréstimo de 600 e
    // retirada de 100.
    lancar_no_razao(&c, "Depreciação", &[("5.4", 20), ("1.4.02", -20)], false);
    lancar_no_razao(&c, "Máquina", &[("1.4.01", 500), ("1.1.01", -500)], true);
    lancar_no_razao(&c, "Empréstimo", &[("1.1.01", 600), ("2.1.04", -600)], true);
    lancar_no_razao(&c, "Retirada", &[("3.3", 100), ("1.1.01", -100)], true);

    let periodo = Periodo::novo(h.inicio_do_mes(), h.fim_do_mes());
    let direto: Dfc = c.consulta(
        &s,
        "financeiro.dfc_do_periodo.v1",
        &DfcDoPeriodo { periodo },
    );
    let ind: DfcIndireto = c.consulta(
        &s,
        "financeiro.dfc_indireto_do_periodo.v1",
        &DfcIndiretoDoPeriodo { periodo },
    );
    let dre: Dre = c.consulta(
        &s,
        "financeiro.dre_do_periodo.v1",
        &DreDoPeriodo { periodo },
    );

    // Caixa: 600 − 150 − 500 + 600 − 100 = 450, nos dois métodos, sem diferença de datas.
    assert_eq!(direto.variacao, Dinheiro::reais(450));
    assert_eq!(ind.variacao, direto.variacao);
    assert_eq!(ind.saldo_final, direto.saldo_final);
    assert_eq!(ind.diferenca_de_datas, Dinheiro::ZERO, "{ind:#?}");
    // Lucro 1.000 − 400 − 20 = 580, o mesmo da DRE.
    assert_eq!(ind.lucro_liquido, dre.lucro_liquido);
    assert_eq!(ind.lucro_liquido, Dinheiro::reais(580));
    assert_eq!(ind.blocos[0].linhas[0].rotulo, LUCRO_LIQUIDO);
    // Operacional: 580 + 20 (depreciação) − 400 (a receber subiu) + 250 (fornecedores subiu).
    assert_eq!(ind.blocos[0].total, Dinheiro::reais(450));
    assert_eq!(ind.blocos[1].total, Dinheiro::reais(-500));
    assert_eq!(ind.blocos[2].total, Dinheiro::reais(500));
}
