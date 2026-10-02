//! Organização com vários CNPJs pelo HTTP (ADR-0017): o admin cadastra uma filial na mesma
//! base, a vê no login, os clientes são do grupo, um funcionário pode estar só num CNPJ, e
//! ninguém de fora cadastra CNPJ na organização dos outros.

mod comum;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use cardeal_kernel::{CodigoErro, Id};
use cardeal_protocol::{
    rota_empresas, rota_membro, rota_membros, EmpresaAcessivel, MembroAdicionado,
    PedidoNovaEmpresa, PedidoNovoMembro, RespostaLogin, TipoCliente,
};
use comum::*;
use mod_clientes::ItemPessoa;
use mod_empresa::PapelDeFabrica;

fn nova_empresa(razao: &str, cnpj: &str) -> Vec<u8> {
    postcard::to_stdvec(&PedidoNovaEmpresa {
        razao_social: razao.into(),
        cnpj: cnpj.into(),
    })
    .unwrap()
}

fn nomes(r: &Resposta) -> Vec<String> {
    r.valor::<Vec<ItemPessoa>>()
        .into_iter()
        .map(|p| p.nome)
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn filial_na_mesma_base_com_clientes_do_grupo_e_acesso_por_cnpj() {
    let a = ambiente(Duration::from_secs(600));
    let ana = token(&a.app, "ana@x.com").await;

    let filial: EmpresaAcessivel = chamar(
        &a.app,
        Method::POST,
        &rota_empresas(a.empresa_ana),
        Some(&ana),
        &[],
        nova_empresa("Loja da Ana — Centro", "11.222.333/0002-62"),
    )
    .await
    .valor();
    let repetida = chamar(
        &a.app,
        Method::POST,
        &rota_empresas(a.empresa_ana),
        Some(&ana),
        &[],
        nova_empresa("De novo", "11222333000262"),
    )
    .await;
    assert_eq!(repetida.erro().codigo, CodigoErro::DUPLICADO);

    // O login já mostra os dois CNPJs.
    let resposta: RespostaLogin = login(&a.app, "ana@x.com", SENHA, TipoCliente::Nativo)
        .await
        .valor();
    let mut ids: Vec<Id> = resposta.empresas.iter().map(|e| e.id).collect();
    ids.sort();
    let mut esperado = vec![a.empresa_ana, filial.id];
    esperado.sort();
    assert_eq!(ids, esperado);

    // Clientes são do grupo — e a mesma base aberta, não duas.
    for (empresa, nome) in [
        (a.empresa_ana, "Cliente da Matriz"),
        (filial.id, "Cliente do Centro"),
    ] {
        comando(
            &a.app,
            &ana,
            empresa,
            "clientes.criar_pessoa.v1",
            &criar_pessoa(nome),
            Id::novo(),
        )
        .await
        .valor::<mod_clientes::PessoaCadastrada>();
    }
    let grupo = vec!["Cliente da Matriz", "Cliente do Centro"];
    assert_eq!(nomes(&clientes(&a.app, &ana, a.empresa_ana).await), grupo);
    assert_eq!(nomes(&clientes(&a.app, &ana, filial.id).await), grupo);
    assert_eq!(
        a.servidor.frota().abertas(),
        1,
        "matriz e filial moram na mesma base"
    );

    // Uma vendedora só da filial: entra nela, não na matriz.
    let bia: MembroAdicionado = chamar(
        &a.app,
        Method::POST,
        &rota_membros(filial.id),
        Some(&ana),
        &[],
        postcard::to_stdvec(&PedidoNovoMembro {
            email: "bia@x.com".into(),
            nome: "Bia".into(),
            papel: PapelDeFabrica::Vendedor,
            senha_inicial: Some("senha-da-bia-1".into()),
        })
        .unwrap(),
    )
    .await
    .valor();
    let t_bia = login(&a.app, "bia@x.com", "senha-da-bia-1", TipoCliente::Nativo)
        .await
        .valor::<RespostaLogin>()
        .token
        .unwrap();
    assert_eq!(
        clientes(&a.app, &t_bia, filial.id).await.status,
        StatusCode::OK
    );
    assert_eq!(
        clientes(&a.app, &t_bia, a.empresa_ana).await.status,
        StatusCode::FORBIDDEN
    );

    // A Bia também vai para a matriz; depois sai da filial — e continua na matriz.
    chamar(
        &a.app,
        Method::POST,
        &rota_membros(a.empresa_ana),
        Some(&ana),
        &[],
        postcard::to_stdvec(&PedidoNovoMembro {
            email: "bia@x.com".into(),
            nome: "Bia".into(),
            papel: PapelDeFabrica::Vendedor,
            senha_inicial: None,
        })
        .unwrap(),
    )
    .await
    .valor::<MembroAdicionado>();
    let saiu = chamar(
        &a.app,
        Method::DELETE,
        &rota_membro(filial.id, bia.usuario),
        Some(&ana),
        &[],
        Vec::new(),
    )
    .await;
    assert_eq!(saiu.status, StatusCode::NO_CONTENT);
    assert_eq!(
        clientes(&a.app, &t_bia, a.empresa_ana).await.status,
        StatusCode::OK
    );
    assert_eq!(
        clientes(&a.app, &t_bia, filial.id).await.status,
        StatusCode::FORBIDDEN
    );

    // E o Beto, de outra organização, não cadastra CNPJ na da Ana.
    let beto = token(&a.app, "beto@x.com").await;
    let intruso = chamar(
        &a.app,
        Method::POST,
        &rota_empresas(a.empresa_ana),
        Some(&beto),
        &[],
        nova_empresa("Golpe", "99.999.999/0001-99"),
    )
    .await;
    assert_eq!(intruso.status, StatusCode::FORBIDDEN);
}
