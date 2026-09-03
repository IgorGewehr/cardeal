//! A identidade rodando contra SQLite de verdade, via `RepositorioAuth` sobre a
//! `UnidadeDeTrabalho` do escritor único.

use cardeal_auth::{
    autorizar, consultas, AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel, PapelDeFabrica,
    PoliticaSenha, RepositorioAuth, Sessao, Usuario, ValorLimite,
};
use cardeal_kernel::{Id, Instante, Percentual};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use tempfile::TempDir;

fn sqlite(e: cardeal_auth::ErroAuth) -> ErroArmazenamento {
    ErroArmazenamento::Sqlite(e.to_string())
}

fn base() -> (TempDir, Armazenamento, Id) {
    let dir = tempfile::tempdir().unwrap();
    let arm =
        Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("cardeal.db"))).unwrap();

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
            Ok(())
        })
        .unwrap();

    (dir, arm, empresa)
}

fn ctx(empresa: Id) -> ContextoEscrita {
    ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()).em(Instante::agora())
}

const SENHA: &str = "Corr3io-Azul-Serra!";
const OUTRA: &str = "Trem-Bao-de-Minas-99";

#[test]
fn usuario_faz_a_volta_completa_pelo_banco() {
    let (_dir, arm, empresa) = base();
    let criado = Usuario::novo(
        "Ana",
        "Ana Prado",
        SENHA,
        PoliticaSenha::PADRAO,
        Instante::EPOCA,
    )
    .unwrap();
    let id = criado.id;

    arm.escritor()
        .executar(ctx(empresa), move |uow| {
            RepositorioAuth::novo(uow)
                .inserir_usuario(&criado)
                .map_err(sqlite)
        })
        .unwrap();

    let lido = arm
        .leitor()
        .consultar(|c| {
            consultas::usuario_por_login(c, "ana")
                .map_err(sqlite)
                .map(|o| o.expect("usuário existe"))
        })
        .unwrap();

    assert_eq!(lido.id, id);
    assert_eq!(lido.login, "ana");
    assert_eq!(lido.nome, "Ana Prado");
    assert!(lido.exige_troca_senha);
    assert!(lido.ativo);
    assert!(cardeal_auth::verificar_senha(SENHA, &lido.hash_senha));
}

#[test]
fn falha_de_autenticacao_persiste_o_bloqueio() {
    let (_dir, arm, empresa) = base();
    let criado =
        Usuario::novo("Bia", "Bia", SENHA, PoliticaSenha::PADRAO, Instante::EPOCA).unwrap();
    arm.escritor()
        .executar(ctx(empresa), {
            let criado = criado.clone();
            move |uow| {
                RepositorioAuth::novo(uow)
                    .inserir_usuario(&criado)
                    .map_err(sqlite)
            }
        })
        .unwrap();

    // Cinco tentativas erradas, cada uma numa unidade de trabalho, gravando o estado.
    for _ in 0..5 {
        arm.escritor()
            .executar(ctx(empresa), move |uow| {
                let mut repo = RepositorioAuth::novo(uow);
                let mut u = repo.usuario_por_login("bia").map_err(sqlite)?.unwrap();
                let _ = u.autenticar("errada", Instante::EPOCA);
                repo.atualizar_credenciais(&u).map_err(sqlite)
            })
            .unwrap();
    }

    let u = arm
        .leitor()
        .consultar(|c| {
            consultas::usuario_por_login(c, "bia")
                .map_err(sqlite)
                .map(Option::unwrap)
        })
        .unwrap();
    assert_eq!(u.bloqueio.tentativas, 5);
    assert!(u.bloqueio.esta_bloqueado(Instante::EPOCA));
}

#[test]
fn troca_de_senha_persiste_e_a_antiga_para_de_valer() {
    let (_dir, arm, empresa) = base();
    let criado = Usuario::novo(
        "Caio",
        "Caio",
        SENHA,
        PoliticaSenha::PADRAO,
        Instante::EPOCA,
    )
    .unwrap();
    arm.escritor()
        .executar(ctx(empresa), {
            let criado = criado.clone();
            move |uow| {
                RepositorioAuth::novo(uow)
                    .inserir_usuario(&criado)
                    .map_err(sqlite)
            }
        })
        .unwrap();

    arm.escritor()
        .executar(ctx(empresa), move |uow| {
            let mut repo = RepositorioAuth::novo(uow);
            let mut u = repo.usuario_por_login("caio").map_err(sqlite)?.unwrap();
            u.trocar_senha(SENHA, OUTRA, PoliticaSenha::PADRAO, Instante::EPOCA)
                .map_err(sqlite)?;
            repo.atualizar_credenciais(&u).map_err(sqlite)
        })
        .unwrap();

    let u = arm
        .leitor()
        .consultar(|c| {
            consultas::usuario_por_login(c, "caio")
                .map_err(sqlite)
                .map(Option::unwrap)
        })
        .unwrap();
    assert!(!u.exige_troca_senha);
    assert!(cardeal_auth::verificar_senha(OUTRA, &u.hash_senha));
    assert!(!cardeal_auth::verificar_senha(SENHA, &u.hash_senha));
}

#[test]
fn papel_editavel_faz_a_volta_com_permissoes_e_limites() {
    let (_dir, arm, empresa) = base();
    let papel = Papel::novo(empresa, "Caixa da frente")
        .com_permissao("vendas.venda.registrar")
        .com_permissao("financeiro.caixa.abrir")
        .com_limite(
            "vendas.desconto_maximo",
            ValorLimite::Percentual(Percentual::pontos(5)),
        )
        .com_limite(
            "financeiro.pagar.teto",
            ValorLimite::Dinheiro(cardeal_kernel::Dinheiro::reais(2_000)),
        );
    let papel_id = papel.id;

    arm.escritor()
        .executar(ctx(empresa), move |uow| {
            RepositorioAuth::novo(uow)
                .inserir_papel(&papel)
                .map_err(sqlite)
        })
        .unwrap();

    let lido = arm
        .leitor()
        .consultar(|c| {
            consultas::papel_por_id(c, papel_id)
                .map_err(sqlite)
                .map(Option::unwrap)
        })
        .unwrap();

    assert_eq!(lido.nome, "Caixa da frente");
    assert_eq!(lido.empresa, Some(empresa));
    assert!(!lido.sistema);
    assert!(lido.concede("vendas.venda.registrar"));
    assert!(lido.concede("financeiro.caixa.abrir"));
    assert_eq!(
        lido.limites.get("vendas.desconto_maximo"),
        Some(&ValorLimite::Percentual(Percentual::pontos(5)))
    );
    assert_eq!(
        lido.limites.get("financeiro.pagar.teto"),
        Some(&ValorLimite::Dinheiro(cardeal_kernel::Dinheiro::reais(
            2_000
        )))
    );
}

#[test]
fn papel_de_fabrica_semeado_nao_e_editavel_pelo_repositorio() {
    let (_dir, arm, _empresa) = base();
    let catalogo = ["vendas.pedido.faturar", "razao.reabrir_periodo"];
    let mut admin = PapelDeFabrica::Administrador.materializar(catalogo);
    let admin_id = admin.id;

    arm.escritor()
        .executar(ctx(Id::novo()), {
            let admin = admin.clone();
            move |uow| {
                RepositorioAuth::novo(uow)
                    .inserir_papel(&admin)
                    .map_err(sqlite)
            }
        })
        .unwrap();

    // O adaptador recusa regravar um papel de fábrica.
    admin.nome = "Administrador editado".to_string();
    let erro = arm
        .escritor()
        .executar(ctx(Id::novo()), move |uow| {
            Ok(RepositorioAuth::novo(uow).atualizar_papel(&admin))
        })
        .unwrap()
        .valor;
    assert!(matches!(erro, Err(cardeal_auth::ErroAuth::PapelDoSistema)));

    let lido = arm
        .leitor()
        .consultar(|c| {
            consultas::papel_por_id(c, admin_id)
                .map_err(sqlite)
                .map(Option::unwrap)
        })
        .unwrap();
    assert_eq!(lido.nome, "Administrador");
    assert!(lido.sistema);
}

#[test]
fn papeis_do_usuario_alimentam_autorizacoes_e_autorizar() {
    let (_dir, arm, empresa) = base();

    let usuario = Usuario::novo(
        "Dora",
        "Dora",
        SENHA,
        PoliticaSenha::PADRAO,
        Instante::EPOCA,
    )
    .unwrap();
    let usuario_id = usuario.id;
    let vendedor = Papel::novo(empresa, "Vendedor")
        .com_permissao("vendas.pedido.criar")
        .com_permissao("clientes.pessoa.ver");
    let vendedor_id = vendedor.id;

    arm.escritor()
        .executar(ctx(empresa), move |uow| {
            let mut repo = RepositorioAuth::novo(uow);
            repo.inserir_usuario(&usuario).map_err(sqlite)?;
            repo.inserir_papel(&vendedor).map_err(sqlite)?;
            repo.atribuir_papel(usuario_id, vendedor_id, empresa)
                .map_err(sqlite)
        })
        .unwrap();

    let papeis = arm
        .leitor()
        .consultar(|c| consultas::papeis_do_usuario(c, usuario_id, empresa).map_err(sqlite))
        .unwrap();
    assert_eq!(papeis.len(), 1);

    let auth = AutorizacoesEfetivas::consolidar(papeis.iter());
    let sessao = Sessao::abrir(EmissaoSessao::padrao(
        usuario_id,
        Id::novo(),
        Escopo::empresa_inteira(empresa),
        auth,
        Instante::EPOCA,
    ));

    assert!(autorizar(
        &sessao,
        "vendas.pedido.criar",
        &Escopo::empresa_inteira(empresa)
    )
    .is_ok());
    assert!(autorizar(&sessao, "razao.estornar", &Escopo::empresa_inteira(empresa)).is_err());
}

#[test]
fn revogar_papel_tira_o_acesso() {
    let (_dir, arm, empresa) = base();
    let usuario =
        Usuario::novo("Eli", "Eli", SENHA, PoliticaSenha::PADRAO, Instante::EPOCA).unwrap();
    let usuario_id = usuario.id;
    let papel = Papel::novo(empresa, "Temp").com_permissao("vendas.pedido.criar");
    let papel_id = papel.id;

    arm.escritor()
        .executar(ctx(empresa), move |uow| {
            let mut repo = RepositorioAuth::novo(uow);
            repo.inserir_usuario(&usuario).map_err(sqlite)?;
            repo.inserir_papel(&papel).map_err(sqlite)?;
            repo.atribuir_papel(usuario_id, papel_id, empresa)
                .map_err(sqlite)?;
            repo.revogar_papel(usuario_id, papel_id, empresa)
                .map_err(sqlite)
        })
        .unwrap();

    let papeis = arm
        .leitor()
        .consultar(|c| consultas::papeis_do_usuario(c, usuario_id, empresa).map_err(sqlite))
        .unwrap();
    assert!(papeis.is_empty());
}
