//! Finanças pessoais (privadas por usuário) e a projeção de custos por categoria, pelo
//! `Despachante` real contra SQLite.

#![allow(clippy::result_large_err)]

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel, Sessao};
use cardeal_kernel::{Competencia, Data, Dinheiro, Fuso, Id, Instante, Periodo};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_financeiro::pessoal::comandos::{ExcluirPessoal, LancarPessoal, MarcarPessoalPago};
use mod_financeiro::pessoal::consultas::LancamentosPessoais;
use mod_financeiro::pessoal::{LancamentoPessoal, NovoPessoal, Repeticao, TipoPessoal};
use mod_financeiro::{
    CategoriaFinanceira, Categorias, CriarCategoriasSugeridas, EspecieTitulo,
    ItemProjecaoCategoria, LancarTituloAPagar, MeioPagamento, ModuloFinanceiro,
    ProjecaoPorCategoria, QuitadoAgora, MANIFESTO,
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

fn ano() -> Periodo {
    Periodo::novo(hoje().mais_dias(-31), hoje().mais_meses(13))
}

#[test]
fn pessoal_e_privado_de_cada_usuario() {
    let c = cena();
    let (eu, outro) = (Id::novo(), Id::novo());
    let s_eu = c.sessao(eu, &["financeiro.pessoal"]);
    let s_outro = c.sessao(outro, &["financeiro.pessoal"]);

    let ids: Vec<Id> = c.cmd(
        &s_eu,
        "financeiro.lancar_pessoal.v1",
        &LancarPessoal(NovoPessoal {
            tipo: TipoPessoal::Despesa,
            descricao: "Parcelamentos do cartão".to_owned(),
            categoria: "Cartão".to_owned(),
            valor: Dinheiro::reais(800),
            primeiro_vencimento: hoje(),
            repeticao: Repeticao::Mensal { meses: 10 },
            cartao: Some("Nubank".to_owned()),
            primeira_paga: false,
        }),
    );
    assert_eq!(ids.len(), 10);

    let meus: Vec<LancamentoPessoal> = c.consulta(
        &s_eu,
        "financeiro.lancamentos_pessoais.v1",
        &LancamentosPessoais { periodo: ano() },
    );
    assert_eq!(meus.len(), 10);
    let dele: Vec<LancamentoPessoal> = c.consulta(
        &s_outro,
        "financeiro.lancamentos_pessoais.v1",
        &LancamentosPessoais { periodo: ano() },
    );
    assert!(dele.is_empty());

    // O outro não consegue marcar nem excluir o que é meu, nem com o id.
    let n: usize = c.cmd(
        &s_outro,
        "financeiro.marcar_pessoal_pago.v1",
        &MarcarPessoalPago {
            lancamentos: vec![ids[0]],
            pago: true,
            data: hoje(),
        },
    );
    assert_eq!(n, 0);

    let n: usize = c.cmd(
        &s_eu,
        "financeiro.marcar_pessoal_pago.v1",
        &MarcarPessoalPago {
            lancamentos: vec![ids[0]],
            pago: true,
            data: hoje(),
        },
    );
    assert_eq!(n, 1);

    // Cancelar a partir da 4ª: sobram 3.
    let n: usize = c.cmd(
        &s_eu,
        "financeiro.excluir_pessoal.v1",
        &ExcluirPessoal {
            lancamento: ids[3],
            e_seguintes: true,
        },
    );
    assert_eq!(n, 7);
    let meus: Vec<LancamentoPessoal> = c.consulta(
        &s_eu,
        "financeiro.lancamentos_pessoais.v1",
        &LancamentosPessoais { periodo: ano() },
    );
    assert_eq!(meus.len(), 3);
    assert!(meus[0].pago());
}

#[test]
fn projecao_separa_custos_por_categoria_e_mes() {
    let c = cena();
    let s = c.sessao(
        Id::novo(),
        &[
            "financeiro.categoria.criar",
            "financeiro.categoria.ver",
            "financeiro.pagar.criar",
            "financeiro.pagar.baixar",
            "financeiro.projecao.ver",
        ],
    );
    let criadas: u32 = c.cmd(
        &s,
        "financeiro.criar_categorias_sugeridas.v1",
        &CriarCategoriasSugeridas,
    );
    assert!(criadas > 10);
    // Rodar de novo não duplica.
    let de_novo: u32 = c.cmd(
        &s,
        "financeiro.criar_categorias_sugeridas.v1",
        &CriarCategoriasSugeridas,
    );
    assert_eq!(de_novo, 0);
    let cats: Vec<CategoriaFinanceira> = c.consulta(&s, "financeiro.categorias.v1", &Categorias);
    let cat = |nome: &str| cats.iter().find(|c| c.nome == nome).map(|c| c.id);

    let pagar = |valor: i64, venc: Data, categoria: Option<Id>, quitado: bool| {
        let _: mod_financeiro::TituloAPagarLancado = c.cmd(
            &s,
            "financeiro.lancar_titulo_a_pagar.v1",
            &LancarTituloAPagar {
                fornecedor: None,
                valor_total: Dinheiro::reais(valor),
                emissao: venc,
                parcelas: 1,
                primeiro_vencimento: venc,
                intervalo_dias: 0,
                observacao: None,
                categoria,
                quitado_agora: quitado.then_some(QuitadoAgora {
                    meio_pagamento: MeioPagamento::Dinheiro,
                    conta: None,
                }),
            },
        );
    };
    let proximo = hoje().inicio_do_mes().mais_meses(1).mais_dias(4);
    pagar(300, hoje(), cat("Energia elétrica"), true);
    pagar(1500, proximo, cat("Aluguel"), false);

    let itens: Vec<ItemProjecaoCategoria> = c.consulta(
        &s,
        "financeiro.projecao_por_categoria.v1",
        &ProjecaoPorCategoria {
            especie: EspecieTitulo::Pagar,
            de: hoje().competencia(),
            meses: 3,
        },
    );
    let este: Competencia = hoje().competencia();
    let energia = itens
        .iter()
        .find(|i| i.categoria == cat("Energia elétrica"))
        .expect("energia");
    assert_eq!(
        (energia.competencia, energia.realizado),
        (este, Dinheiro::reais(300))
    );
    let aluguel = itens
        .iter()
        .find(|i| i.categoria == cat("Aluguel"))
        .expect("aluguel");
    assert_eq!(
        (aluguel.competencia, aluguel.em_aberto),
        (este.proxima(), Dinheiro::reais(1500))
    );
}
