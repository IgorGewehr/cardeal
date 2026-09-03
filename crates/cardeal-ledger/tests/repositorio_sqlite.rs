//! O Razão rodando contra SQLite de verdade, via `RepositorioRazao` sobre a
//! `UnidadeDeTrabalho` do escritor único.

use cardeal_kernel::{Data, Dinheiro, Fuso, Id, Instante};
use cardeal_ledger::{
    migracoes, semear_plano_padrao, ConstrutorLancamento, Contas, ErroRazao, EstadoLancamento,
    PapelConta, PortaRazao, Razao, RepositorioRazao,
};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use tempfile::TempDir;

fn liga(e: ErroRazao) -> ErroArmazenamento {
    ErroArmazenamento::Sqlite(e.to_string())
}

fn base() -> (TempDir, Armazenamento, Id) {
    let dir = tempfile::tempdir().unwrap();
    let arm =
        Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("cardeal.db"))).unwrap();
    arm.migrar(&[migracoes::conjunto()]).unwrap();

    let empresa = Id::novo();
    let ctx = ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx, move |uow| {
            uow.conexao()
                .execute(
                    "INSERT INTO nucleo_empresa
                       (id, razao_social, nome_fantasia, cnpj, regime, endereco, perfil, criado_em)
                     VALUES (?1, 'Teste LTDA', 'Teste', '11222333000181', 'SimplesNacional', '{}', 'comercio', 0)",
                    [empresa.em_bytes().as_slice()],
                )
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
            semear_plano_padrao(uow, empresa).map_err(liga)
        })
        .unwrap();

    (dir, arm, empresa)
}

fn ctx(empresa: Id) -> ContextoEscrita {
    ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()).em(Instante::agora())
}

fn hoje() -> Data {
    Data::hoje(Fuso::BRASILIA)
}

/// Registra uma venda em espécie (D Caixa / C Receita) no lançamento e devolve o id.
fn lancar_venda(arm: &Armazenamento, empresa: Id, reais: i64) -> Id {
    arm.escritor()
        .executar(ctx(empresa), move |uow| {
            let mut repo = RepositorioRazao::novo(uow);
            let (caixa, receita) = {
                let contas = Contas::nova(&repo, empresa);
                (
                    contas.papel(PapelConta::Caixa).map_err(liga)?,
                    contas.papel(PapelConta::ReceitaVendas).map_err(liga)?,
                )
            };
            let lanc = ConstrutorLancamento::novo(empresa, hoje(), "Venda de teste")
                .liquidacao(hoje())
                .criado_por(repo.usuario(), repo.dispositivo())
                .debitar(caixa, Dinheiro::reais(reais))
                .creditar(receita, Dinheiro::reais(reais))
                .construir()
                .map_err(liga)?;
            Razao::registrar(&mut repo, lanc).map_err(liga)
        })
        .unwrap()
        .valor
}

#[test]
fn registra_e_le_um_lancamento_real() {
    let (_dir, arm, empresa) = base();
    let id = lancar_venda(&arm, empresa, 100);

    let (numero, estado, partidas, balanceado) = arm
        .escritor()
        .executar(ctx(empresa), move |uow| {
            let repo = RepositorioRazao::novo(uow);
            let l = repo
                .buscar_lancamento(id)
                .map_err(liga)?
                .expect("lançamento existe");
            Ok((l.numero, l.estado, l.partidas.len(), l.esta_balanceado()))
        })
        .unwrap()
        .valor;

    assert_eq!(numero, 1);
    assert_eq!(estado, EstadoLancamento::Realizado);
    assert_eq!(partidas, 2);
    assert!(balanceado);
}

#[test]
fn numero_e_sequencial_e_persiste_entre_unidades_de_trabalho() {
    let (_dir, arm, empresa) = base();
    lancar_venda(&arm, empresa, 10);
    lancar_venda(&arm, empresa, 10);
    let terceiro = lancar_venda(&arm, empresa, 10);

    let numero = arm
        .escritor()
        .executar(ctx(empresa), move |uow| {
            let repo = RepositorioRazao::novo(uow);
            Ok(repo
                .buscar_lancamento(terceiro)
                .map_err(liga)?
                .unwrap()
                .numero)
        })
        .unwrap()
        .valor;
    assert_eq!(numero, 3);
}

#[test]
fn estorno_marca_o_original_e_grava_o_espelho() {
    let (_dir, arm, empresa) = base();
    let venda = lancar_venda(&arm, empresa, 100);

    let (estado_original, estorno_balanceado) = arm
        .escritor()
        .executar(ctx(empresa), move |uow| {
            let mut repo = RepositorioRazao::novo(uow);
            let estorno_id =
                Razao::estornar(&mut repo, venda, "cliente devolveu a mercadoria", hoje())
                    .map_err(liga)?;
            let original = repo.buscar_lancamento(venda).map_err(liga)?.unwrap();
            let estorno = repo.buscar_lancamento(estorno_id).map_err(liga)?.unwrap();
            Ok((original.estado, estorno.esta_balanceado()))
        })
        .unwrap()
        .valor;

    assert_eq!(estado_original, EstadoLancamento::Estornado);
    assert!(estorno_balanceado);
}
