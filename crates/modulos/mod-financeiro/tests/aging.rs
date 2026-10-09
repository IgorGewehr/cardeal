//! O aging de recebíveis e pagáveis lido das parcelas reais, pelo `Despachante`.

#![allow(clippy::result_large_err)]

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel, Sessao};
use cardeal_kernel::{Data, Dinheiro, Fuso, Id, Instante};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_financeiro::aging::Aging;
use mod_financeiro::{LancarTituloAReceber, ModuloFinanceiro, MANIFESTO};
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

fn receber(c: &Cena, s: &Sessao, valor: i64, vencimento: Data, cliente: Option<Id>) {
    let _: mod_financeiro::TituloAReceberLancado = c.cmd(
        s,
        "financeiro.lancar_titulo_a_receber.v1",
        &LancarTituloAReceber {
            cliente,
            valor_total: Dinheiro::reais(valor),
            emissao: vencimento.mais_dias(-30),
            parcelas: 1,
            primeiro_vencimento: vencimento,
            intervalo_dias: 0,
            observacao: None,
            categoria: None,
            quitado_agora: None,
        },
    );
}

#[test]
fn aging_separa_a_vencer_de_vencido_e_aponta_o_maior_devedor() {
    let c = cena();
    let s = c.sessao(
        Id::novo(),
        &[
            "financeiro.receber.criar",
            "financeiro.receber.ver",
            "financeiro.pagar.ver",
        ],
    );
    let devedor = Id::novo();
    receber(&c, &s, 500, hoje().mais_dias(10), None);
    receber(&c, &s, 300, hoje().mais_dias(-5), Some(devedor));
    receber(&c, &s, 700, hoje().mais_dias(-50), Some(devedor));

    let a: Aging = c.consulta(
        &s,
        "financeiro.aging_a_receber.v1",
        &mod_financeiro::AgingAReceber,
    );
    assert_eq!(a.total_aberto, Dinheiro::reais(1_500));
    assert_eq!(a.total_vencido, Dinheiro::reais(1_000));
    assert_eq!(a.vencido_mais_de_30, Dinheiro::reais(700));
    assert_eq!(a.faixas[0].valor, Dinheiro::reais(500));
    assert_eq!(a.faixas[1].valor, Dinheiro::reais(300));
    assert_eq!(a.faixas[2].valor, Dinheiro::reais(700));
    assert_eq!(a.devedores.len(), 1);
    assert_eq!(a.devedores[0].vencido, Dinheiro::reais(1_000));
    assert_eq!(a.devedores[0].dias_do_mais_antigo, 50);

    // O aging a pagar é outra permissão e outra espécie: nada lançado.
    let p: Aging = c.consulta(
        &s,
        "financeiro.aging_a_pagar.v1",
        &mod_financeiro::AgingAPagar,
    );
    assert_eq!(p.total_aberto, Dinheiro::ZERO);
}
