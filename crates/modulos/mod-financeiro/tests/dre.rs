//! A DRE gerencial lida do Razão real, pelo `Despachante`.

#![allow(clippy::result_large_err)]

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel, Sessao};
use cardeal_kernel::{Data, Dinheiro, Fuso, Id, Instante, Periodo};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_financeiro::dre::Dre;
use mod_financeiro::{
    ComparativoDre, DreComparativa, DreDoPeriodo, LancarTituloAPagar, LancarTituloAReceber,
    ModuloFinanceiro, MANIFESTO,
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
