//! A conciliação bancária (importar OFX, sugerir, conciliar, lançar, ignorar) pelo `Despachante`.

#![allow(clippy::result_large_err)]

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel, Sessao};
use cardeal_kernel::{CodigoErro, Data, Dinheiro, Fuso, Id, Instante};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_financeiro::extrato::FormatoExtrato;
use mod_financeiro::sugestao::AlvoDaConciliacao;
use mod_financeiro::{
    BaixarRecebimento, ConciliarItem, ContaBancariaCriada, CriarContaBancaria, EstadoItem,
    ExtratoImportado, IgnorarItemDoExtrato, ImportarExtrato, ItemDoExtrato, ItensDoExtrato,
    LancarItemDoExtrato, LancarTituloAPagar, LancarTituloAReceber, MeioPagamento, ModuloFinanceiro,
    MANIFESTO,
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

const PERMS: &[&str] = &[
    "financeiro.banco.criar",
    "financeiro.receber.criar",
    "financeiro.receber.baixar",
    "financeiro.pagar.criar",
    "financeiro.pagar.baixar",
    "financeiro.conciliacao.ver",
    "financeiro.conciliacao.importar",
    "financeiro.conciliacao.confirmar",
];

fn ofx(dia: Data) -> String {
    let d = dia.formatar_iso().replace('-', "");
    format!(
        "<OFX><BANKTRANLIST>\n\
<STMTTRN><DTPOSTED>{d}<TRNAMT>340.00<FITID>F1<MEMO>PIX RECEBIDO JOAO SILVA\n\
<STMTTRN><DTPOSTED>{d}<TRNAMT>-500.00<FITID>F2<MEMO>TED FORNECEDOR ALFA\n\
<STMTTRN><DTPOSTED>{d}<TRNAMT>-29.90<FITID>F3<MEMO>TARIFA PACOTE SERVICOS\n\
<STMTTRN><DTPOSTED>{d}<TRNAMT>12.00<FITID>F4<MEMO>RENDIMENTO POUPANCA\n\
</BANKTRANLIST></OFX>"
    )
}

struct Cenario {
    c: Cena,
    s: Sessao,
    banco: Id,
}

fn montar() -> Cenario {
    let c = cena();
    let s = c.sessao(Id::novo(), PERMS);
    let b: ContaBancariaCriada = c.cmd(
        &s,
        "financeiro.criar_conta_bancaria.v1",
        &CriarContaBancaria {
            nome: "Nubank".into(),
        },
    );
    Cenario {
        c,
        s,
        banco: b.conta,
    }
}

impl Cenario {
    fn itens(&self, estado: Option<EstadoItem>) -> Vec<ItemDoExtrato> {
        self.c.consulta(
            &self.s,
            "financeiro.itens_do_extrato.v1",
            &ItensDoExtrato {
                conta: self.banco,
                estado,
            },
        )
    }

    fn importar(&self) -> ExtratoImportado {
        self.c.cmd(
            &self.s,
            "financeiro.importar_extrato.v1",
            &ImportarExtrato {
                conta: self.banco,
                formato: FormatoExtrato::Ofx,
                arquivo: "extrato.ofx".into(),
                conteudo: ofx(hoje()),
            },
        )
    }

    fn por_descricao(&self, trecho: &str) -> ItemDoExtrato {
        self.itens(None)
            .into_iter()
            .find(|i| i.descricao.contains(trecho))
            .unwrap_or_else(|| panic!("linha {trecho}"))
    }

    fn tenta<T: serde::Serialize>(
        &self,
        nome: &str,
        c: &T,
    ) -> Result<Vec<u8>, cardeal_kernel::Erro> {
        self.c.d.executar_comando(
            nome,
            &postcard::to_stdvec(c).unwrap(),
            &self.s,
            &self.c.ambiente(),
            self.c.arm.escritor(),
        )
    }
}

#[test]
fn importar_traz_as_linhas_e_reimportar_nao_duplica() {
    let k = montar();
    let r = k.importar();
    assert_eq!((r.importados, r.repetidos), (4, 0));
    assert_eq!(k.itens(None).len(), 4);
    let r2 = k.importar();
    assert_eq!(
        (r2.importados, r2.repetidos),
        (0, 4),
        "períodos que se sobrepõem são o normal"
    );
    assert_eq!(k.itens(None).len(), 4);
    // Arquivo sem lançamentos e conta que não é bancária: recusados.
    let e = k
        .tenta(
            "financeiro.importar_extrato.v1",
            &ImportarExtrato {
                conta: k.banco,
                formato: FormatoExtrato::Ofx,
                arquivo: "x.ofx".into(),
                conteudo: "<OFX></OFX>".into(),
            },
        )
        .unwrap_err();
    assert_eq!(e.codigo, CodigoErro::ENTRADA_INVALIDA);
    let plano: Vec<mod_financeiro::NoPlano> = {
        let s = k.c.sessao(Id::novo(), &["financeiro.plano.ver"]);
        k.c.consulta(
            &s,
            "financeiro.plano_de_contas.v1",
            &mod_financeiro::PlanoDeContas,
        )
    };
    let receita = plano.iter().find(|n| n.codigo == "4.1").unwrap().id;
    let e = k
        .tenta(
            "financeiro.importar_extrato.v1",
            &ImportarExtrato {
                conta: receita,
                formato: FormatoExtrato::Ofx,
                arquivo: "x.ofx".into(),
                conteudo: ofx(hoje()),
            },
        )
        .unwrap_err();
    assert_eq!(e.codigo, CodigoErro::REGRA_VIOLADA);
}

#[test]
fn sugere_baixa_ja_feita_e_parcela_em_aberto_e_concilia_as_duas() {
    let k = montar();
    let h = hoje();
    // O cliente pagou 340 e a baixa já foi dada no banco; o fornecedor de 500 ainda está em aberto.
    let r: mod_financeiro::TituloAReceberLancado = k.c.cmd(
        &k.s,
        "financeiro.lancar_titulo_a_receber.v1",
        &LancarTituloAReceber {
            cliente: None,
            valor_total: Dinheiro::reais(340),
            emissao: h,
            parcelas: 1,
            primeiro_vencimento: h,
            intervalo_dias: 0,
            observacao: None,
            categoria: None,
            quitado_agora: None,
        },
    );
    let baixa: mod_financeiro::RecebimentoBaixado = k.c.cmd(
        &k.s,
        "financeiro.baixar_recebimento.v1",
        &BaixarRecebimento {
            parcela: r.parcelas[0],
            valor: Dinheiro::reais(340),
            data: h,
            meio_pagamento: MeioPagamento::Pix,
            conta_destino: Some(k.banco),
        },
    );
    let p: mod_financeiro::TituloAPagarLancado = k.c.cmd(
        &k.s,
        "financeiro.lancar_titulo_a_pagar.v1",
        &LancarTituloAPagar {
            fornecedor: None,
            valor_total: Dinheiro::reais(500),
            emissao: h,
            parcelas: 1,
            primeiro_vencimento: h,
            intervalo_dias: 0,
            observacao: Some("Alfa".into()),
            categoria: None,
            quitado_agora: None,
        },
    );
    k.importar();

    let entrada = k.por_descricao("JOAO SILVA");
    assert_eq!(entrada.sugestoes.len(), 1);
    assert_eq!(
        entrada.sugestoes[0].alvo,
        AlvoDaConciliacao::Baixa(baixa.baixa)
    );
    assert!(entrada.sugestoes[0].confianca >= 95);
    let saida = k.por_descricao("ALFA");
    assert_eq!(
        saida.sugestoes[0].alvo,
        AlvoDaConciliacao::Parcela(p.parcelas[0])
    );
    assert!(
        k.por_descricao("TARIFA").sugestoes.is_empty(),
        "sem correspondência"
    );

    // Casar com o alvo errado (a parcela de 500 com a linha de 340) é recusado.
    let e = k
        .tenta(
            "financeiro.conciliar_item.v1",
            &ConciliarItem {
                item: entrada.id,
                alvo: AlvoDaConciliacao::Baixa(Id::novo()),
            },
        )
        .unwrap_err();
    assert_eq!(e.codigo, CodigoErro::REGRA_VIOLADA);

    let _: () = k.c.cmd(
        &k.s,
        "financeiro.conciliar_item.v1",
        &ConciliarItem {
            item: entrada.id,
            alvo: entrada.sugestoes[0].alvo,
        },
    );
    let _: () = k.c.cmd(
        &k.s,
        "financeiro.conciliar_item.v1",
        &ConciliarItem {
            item: saida.id,
            alvo: saida.sugestoes[0].alvo,
        },
    );
    // A conciliação da parcela deu a baixa: a parcela deixou de estar em aberto.
    let abertas: Vec<mod_financeiro::ItemTituloEmAberto> = {
        let s = k.c.sessao(Id::novo(), &["financeiro.pagar.ver"]);
        k.c.consulta(
            &s,
            "financeiro.titulos_a_pagar_em_aberto.v1",
            &mod_financeiro::TitulosAPagarEmAberto,
        )
    };
    assert!(abertas.is_empty());
    let conciliados = k.itens(Some(EstadoItem::Conciliado));
    assert_eq!(conciliados.len(), 2);
    assert!(conciliados.iter().all(|i| i.baixa.is_some()));
    // Conciliar de novo a mesma linha: recusado.
    let e = k
        .tenta(
            "financeiro.conciliar_item.v1",
            &ConciliarItem {
                item: saida.id,
                alvo: saida.sugestoes[0].alvo,
            },
        )
        .unwrap_err();
    assert_eq!(e.codigo, CodigoErro::ESTADO_INVALIDO);
}

#[test]
fn tarifa_vira_despesa_quitada_e_rendimento_pode_ser_ignorado_e_voltar() {
    let k = montar();
    k.importar();
    let tarifa = k.por_descricao("TARIFA");
    let _: () = k.c.cmd(
        &k.s,
        "financeiro.lancar_item_do_extrato.v1",
        &LancarItemDoExtrato {
            item: tarifa.id,
            categoria: None,
            descricao: None,
        },
    );
    let depois = k.por_descricao("TARIFA");
    assert_eq!(depois.estado, EstadoItem::Conciliado);
    assert!(depois.baixa.is_some());

    let rendimento = k.por_descricao("RENDIMENTO");
    let _: () = k.c.cmd(
        &k.s,
        "financeiro.ignorar_item_do_extrato.v1",
        &IgnorarItemDoExtrato {
            item: rendimento.id,
            ignorar: true,
        },
    );
    assert_eq!(k.itens(Some(EstadoItem::Ignorado)).len(), 1);
    let _: () = k.c.cmd(
        &k.s,
        "financeiro.ignorar_item_do_extrato.v1",
        &IgnorarItemDoExtrato {
            item: rendimento.id,
            ignorar: false,
        },
    );
    assert_eq!(k.itens(Some(EstadoItem::Pendente)).len(), 3);
    // Ignorar uma conciliada não pode.
    let e = k
        .tenta(
            "financeiro.ignorar_item_do_extrato.v1",
            &IgnorarItemDoExtrato {
                item: tarifa.id,
                ignorar: true,
            },
        )
        .unwrap_err();
    assert_eq!(e.codigo, CodigoErro::ESTADO_INVALIDO);

    // O DFC enxerga o pagamento da tarifa saindo do caixa/banco.
    let s = k.c.sessao(Id::novo(), &["financeiro.dre.ver"]);
    let h = hoje();
    let dfc: mod_financeiro::dfc::Dfc = k.c.consulta(
        &s,
        "financeiro.dfc_do_periodo.v1",
        &mod_financeiro::DfcDoPeriodo {
            periodo: cardeal_kernel::Periodo::novo(h.inicio_do_mes(), h.fim_do_mes()),
        },
    );
    assert_eq!(dfc.variacao, Dinheiro::centavos(-2_990));
}

#[test]
fn sem_permissao_de_confirmar_nada_e_casado() {
    let k = montar();
    k.importar();
    let sem = k.c.sessao(Id::novo(), &["financeiro.conciliacao.ver"]);
    let item = k.por_descricao("TARIFA");
    let e =
        k.c.d
            .executar_comando(
                "financeiro.ignorar_item_do_extrato.v1",
                &postcard::to_stdvec(&IgnorarItemDoExtrato {
                    item: item.id,
                    ignorar: true,
                })
                .unwrap(),
                &sem,
                &k.c.ambiente(),
                k.c.arm.escritor(),
            )
            .unwrap_err();
    assert_eq!(e.codigo, CodigoErro::SEM_PERMISSAO);
}

#[test]
fn saldo_do_banco_contra_o_sistema_com_a_diferenca_explicada_e_os_alertas() {
    use mod_financeiro::{
        ConferenciaDaConta, ConferenciaDasContas, InformarSaldoDoBanco, OrigemDoSaldo,
    };

    let k = montar();
    let h = hoje();
    let conferencia = |k: &Cenario| -> ConferenciaDaConta {
        let todas: Vec<ConferenciaDaConta> = k.c.consulta(
            &k.s,
            "financeiro.conferencia_das_contas.v1",
            &ConferenciaDasContas,
        );
        todas
            .into_iter()
            .find(|c| c.conta == k.banco)
            .expect("a conta")
    };
    // Antes de qualquer extrato: nada a conferir, nada a alertar.
    let c = conferencia(&k);
    assert!(c.fechamento.is_none() && c.ultima_linha.is_none());
    assert!(c.alertas(h, 7).is_empty());

    // Um recebimento de 100 já baixado no banco, no sistema.
    let r: mod_financeiro::TituloAReceberLancado = k.c.cmd(
        &k.s,
        "financeiro.lancar_titulo_a_receber.v1",
        &LancarTituloAReceber {
            cliente: None,
            valor_total: Dinheiro::reais(100),
            emissao: h,
            parcelas: 1,
            primeiro_vencimento: h,
            intervalo_dias: 0,
            observacao: None,
            categoria: None,
            quitado_agora: None,
        },
    );
    let _: mod_financeiro::RecebimentoBaixado = k.c.cmd(
        &k.s,
        "financeiro.baixar_recebimento.v1",
        &BaixarRecebimento {
            parcela: r.parcelas[0],
            valor: Dinheiro::reais(100),
            data: h,
            meio_pagamento: MeioPagamento::Pix,
            conta_destino: Some(k.banco),
        },
    );

    // O banco: +100 (o recebimento), +200 de outro PIX, −50 de tarifa e +5 antigo, com o saldo
    // de 255 informado no LEDGERBAL.
    let iso = |d: Data| d.formatar_iso().replace('-', "");
    let (d0, d1, d10) = (iso(h), iso(h.mais_dias(-1)), iso(h.mais_dias(-10)));
    let ofx = format!(
        "<OFX><STMTRS><BANKTRANLIST>\n\
<STMTTRN><DTPOSTED>{d1}<TRNAMT>100.00<FITID>G1<MEMO>PIX CLIENTE\n\
<STMTTRN><DTPOSTED>{d0}<TRNAMT>200.00<FITID>G2<MEMO>PIX OUTRO\n\
<STMTTRN><DTPOSTED>{d0}<TRNAMT>-50.00<FITID>G3<MEMO>TARIFA\n\
<STMTTRN><DTPOSTED>{d10}<TRNAMT>5.00<FITID>G4<MEMO>ANTIGA\n\
</BANKTRANLIST><LEDGERBAL><BALAMT>255.00<DTASOF>{d0}</LEDGERBAL></STMTRS></OFX>"
    );
    let _: ExtratoImportado = k.c.cmd(
        &k.s,
        "financeiro.importar_extrato.v1",
        &ImportarExtrato {
            conta: k.banco,
            formato: FormatoExtrato::Ofx,
            arquivo: "extrato.ofx".into(),
            conteudo: ofx,
        },
    );

    // Banco 255 × sistema 100: as 4 linhas pendentes (+255) estão só no banco e a baixa de
    // 100 está só no sistema (ainda sem linha conciliada) — nada sem explicação.
    let f = conferencia(&k).fechamento.expect("o OFX trouxe o saldo");
    assert_eq!(f.banco.origem, OrigemDoSaldo::Extrato);
    assert_eq!(f.banco.saldo, Dinheiro::reais(255));
    assert_eq!(f.sistema, Dinheiro::reais(100));
    assert_eq!(
        (f.pendentes.quantidade, f.pendentes.valor),
        (4, Dinheiro::reais(255))
    );
    assert_eq!(f.so_no_sistema.valor, Dinheiro::reais(100));
    assert_eq!(f.sem_explicacao(), Dinheiro::ZERO);

    // Concilia o recebimento e ignora a tarifa: continua tudo explicado.
    let cliente = k.por_descricao("PIX CLIENTE");
    let _: () = k.c.cmd(
        &k.s,
        "financeiro.conciliar_item.v1",
        &ConciliarItem {
            item: cliente.id,
            alvo: cliente.sugestoes[0].alvo,
        },
    );
    let _: () = k.c.cmd(
        &k.s,
        "financeiro.ignorar_item_do_extrato.v1",
        &IgnorarItemDoExtrato {
            item: k.por_descricao("TARIFA").id,
            ignorar: true,
        },
    );
    let c = conferencia(&k);
    let f = c.fechamento.unwrap();
    assert_eq!(f.so_no_sistema.quantidade, 0);
    assert_eq!(f.ignoradas.valor, Dinheiro::reais(-50));
    assert_eq!(f.pendentes.valor, Dinheiro::reais(205));
    assert_eq!(f.sem_explicacao(), Dinheiro::ZERO);
    // A linha de 10 dias atrás vira alerta com tolerância de 7.
    let alertas = c.alertas(h, 7);
    assert_eq!(alertas.len(), 1, "{alertas:?}");
    assert!(alertas[0].contains("10 dias"));

    // O saldo digitado (o aplicativo do banco mostra 305) vence o do OFX na mesma data e deixa
    // 50 sem explicação.
    let _: () = k.c.cmd(
        &k.s,
        "financeiro.informar_saldo_do_banco.v1",
        &InformarSaldoDoBanco {
            conta: k.banco,
            data: h,
            saldo: Dinheiro::reais(305),
        },
    );
    let c = conferencia(&k);
    let f = c.fechamento.unwrap();
    assert_eq!(f.banco.origem, OrigemDoSaldo::Digitado);
    assert_eq!(f.sem_explicacao(), Dinheiro::reais(50));
    assert!(c.alertas(h, 30)[0].contains("50,00"));

    // Saldo com data futura: recusado.
    let e = k
        .tenta(
            "financeiro.informar_saldo_do_banco.v1",
            &InformarSaldoDoBanco {
                conta: k.banco,
                data: h.mais_dias(1),
                saldo: Dinheiro::ZERO,
            },
        )
        .unwrap_err();
    assert_eq!(e.codigo, CodigoErro::ENTRADA_INVALIDA);
}
