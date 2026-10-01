//! O servidor de ponta a ponta, por HTTP de verdade (roteador axum, sem socket): login,
//! comando e consulta de um módulo real, idempotência, isolamento entre empresas, logout e o
//! ciclo de vida da frota (abrir sob demanda → despejar → reabrir).

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use cardeal_kernel::{CodigoErro, Erro, Id};
use cardeal_protocol::{
    rota_comando, rota_consulta, PedidoLogin, RespostaLogin, TipoCliente, CABECALHO_IDEMPOTENCIA,
    CABECALHO_PROTOCOLO, ROTA_SESSAO, TIPO_POSTCARD, VERSAO,
};
use cardeal_server::provisionamento::{provisionar, NovaEmpresa};
use cardeal_server::{roteador, ConfigServidor, Servidor};
use mod_clientes::{CriarPessoa, ItemPessoa, Papel, PessoasPorPapel, TipoPessoa};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tower::ServiceExt as _;

const SENHA: &str = "senha-forte-123";

struct Ambiente {
    _pasta: tempfile::TempDir,
    servidor: Arc<Servidor>,
    app: Router,
    empresa_ana: Id,
    empresa_beto: Id,
}

fn ambiente(ociosidade: Duration) -> Ambiente {
    ambiente_com(|c| c.ociosidade = ociosidade)
}

fn ambiente_com(ajuste: impl FnOnce(&mut ConfigServidor)) -> Ambiente {
    let pasta = tempfile::tempdir().unwrap();
    let mut config = ConfigServidor::em(pasta.path());
    ajuste(&mut config);
    let servidor = Servidor::abrir(config).unwrap();
    let nova = |empresa: &str, email: &str| NovaEmpresa {
        razao_social: empresa.into(),
        cnpj: "11222333000181".into(),
        email: email.into(),
        nome: "Pessoa".into(),
        senha: SENHA.into(),
    };
    let a = provisionar(
        servidor.config(),
        servidor.plano(),
        servidor.diretorio(),
        &nova("Loja da Ana", "ana@x.com"),
    )
    .unwrap();
    let b = provisionar(
        servidor.config(),
        servidor.plano(),
        servidor.diretorio(),
        &nova("Loja do Beto", "beto@x.com"),
    )
    .unwrap();
    Ambiente {
        _pasta: pasta,
        app: roteador(Arc::clone(&servidor)),
        servidor,
        empresa_ana: a.empresa,
        empresa_beto: b.empresa,
    }
}

struct Resposta {
    status: StatusCode,
    cabecalhos: axum::http::HeaderMap,
    corpo: Vec<u8>,
}

impl Resposta {
    fn valor<T: DeserializeOwned>(&self) -> T {
        assert_eq!(
            self.status,
            StatusCode::OK,
            "esperava 200, veio {}: {:?}",
            self.status,
            self.erro_opcional()
        );
        postcard::from_bytes(&self.corpo).unwrap()
    }
    fn erro_opcional(&self) -> Option<Erro> {
        postcard::from_bytes(&self.corpo).ok()
    }
    fn erro(&self) -> Erro {
        self.erro_opcional().expect("corpo de erro em postcard")
    }
}

async fn chamar(
    app: &Router,
    metodo: Method,
    uri: &str,
    token: Option<&str>,
    extras: &[(&str, String)],
    corpo: Vec<u8>,
) -> Resposta {
    let mut req = Request::builder()
        .method(metodo)
        .uri(uri)
        .header(header::CONTENT_TYPE, TIPO_POSTCARD)
        .header(CABECALHO_PROTOCOLO, VERSAO.to_string());
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    for (k, v) in extras {
        req = req.header(*k, v);
    }
    let resp = app
        .clone()
        .oneshot(req.body(Body::from(corpo)).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let cabecalhos = resp.headers().clone();
    let corpo = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    Resposta {
        status,
        cabecalhos,
        corpo,
    }
}

async fn login(app: &Router, email: &str, senha: &str, cliente: TipoCliente) -> Resposta {
    let pedido = PedidoLogin {
        email: email.into(),
        senha: senha.into(),
        cliente,
    };
    chamar(
        app,
        Method::POST,
        ROTA_SESSAO,
        None,
        &[],
        postcard::to_stdvec(&pedido).unwrap(),
    )
    .await
}

async fn token(app: &Router, email: &str) -> String {
    login(app, email, SENHA, TipoCliente::Nativo)
        .await
        .valor::<RespostaLogin>()
        .token
        .unwrap()
}

fn criar_pessoa(nome: &str) -> CriarPessoa {
    CriarPessoa {
        tipo: TipoPessoa::Fisica,
        nome: nome.into(),
        nome_fantasia: None,
        papel_inicial: Papel::Cliente,
        documento_tipo: None,
        documento_numero: None,
        data_nascimento: None,
        endereco: None,
        contato: None,
    }
}

async fn comando<C: Serialize>(
    app: &Router,
    token: &str,
    empresa: Id,
    nome: &str,
    c: &C,
    chave: Id,
) -> Resposta {
    chamar(
        app,
        Method::POST,
        &rota_comando(empresa, nome),
        Some(token),
        &[(CABECALHO_IDEMPOTENCIA, chave.to_string())],
        postcard::to_stdvec(c).unwrap(),
    )
    .await
}

async fn clientes(app: &Router, token: &str, empresa: Id) -> Resposta {
    let q = PessoasPorPapel {
        papel: Papel::Cliente,
        busca: None,
    };
    chamar(
        app,
        Method::POST,
        &rota_consulta(empresa, "clientes.pessoas_por_papel.v1"),
        Some(token),
        &[],
        postcard::to_stdvec(&q).unwrap(),
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn login_nativo_devolve_token_e_navegador_so_recebe_cookie_httponly() {
    let a = ambiente(Duration::from_secs(600));

    let nativo: RespostaLogin = login(&a.app, "ANA@x.com", SENHA, TipoCliente::Nativo)
        .await
        .valor();
    assert!(nativo.token.is_some());
    assert_eq!(nativo.empresas.len(), 1);
    assert_eq!(nativo.empresas[0].id, a.empresa_ana);
    assert_eq!(nativo.empresas[0].nome, "Loja da Ana");

    let r = login(&a.app, "ana@x.com", SENHA, TipoCliente::Navegador).await;
    let navegador: RespostaLogin = r.valor();
    assert!(
        navegador.token.is_none(),
        "o token nunca vai no corpo para o navegador"
    );
    let cookie = r
        .cabecalhos
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    for atributo in ["cardeal_sessao=", "HttpOnly", "Secure", "SameSite=Strict"] {
        assert!(cookie.contains(atributo), "faltou {atributo} em {cookie}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn senha_errada_e_email_inexistente_respondem_igual() {
    let a = ambiente(Duration::from_secs(600));
    let errada = login(
        &a.app,
        "ana@x.com",
        "outra-senha-qualquer",
        TipoCliente::Nativo,
    )
    .await;
    let inexistente = login(&a.app, "ninguem@x.com", SENHA, TipoCliente::Nativo).await;
    assert_eq!(errada.status, StatusCode::UNAUTHORIZED);
    assert_eq!(inexistente.status, StatusCode::UNAUTHORIZED);
    assert_eq!(errada.erro().mensagem, inexistente.erro().mensagem);
    assert_eq!(errada.erro().codigo, CodigoErro::CREDENCIAL_INVALIDA);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cinco_senhas_erradas_bloqueiam_a_conta() {
    let a = ambiente(Duration::from_secs(600));
    for _ in 0..5 {
        login(&a.app, "ana@x.com", "errada-errada", TipoCliente::Nativo).await;
    }
    let certa = login(&a.app, "ana@x.com", SENHA, TipoCliente::Nativo).await;
    assert_eq!(certa.erro().codigo, CodigoErro::CONTA_BLOQUEADA);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comando_e_consulta_de_modulo_real_e_reenvio_idempotente() {
    let a = ambiente(Duration::from_secs(600));
    let t = token(&a.app, "ana@x.com").await;
    let chave = Id::novo();

    let primeira = comando(
        &a.app,
        &t,
        a.empresa_ana,
        "clientes.criar_pessoa.v1",
        &criar_pessoa("Maria"),
        chave,
    )
    .await;
    let reenvio = comando(
        &a.app,
        &t,
        a.empresa_ana,
        "clientes.criar_pessoa.v1",
        &criar_pessoa("Maria"),
        chave,
    )
    .await;
    assert_eq!(
        primeira.status,
        StatusCode::OK,
        "{:?}",
        primeira.erro_opcional()
    );
    assert_eq!(
        primeira.corpo, reenvio.corpo,
        "reenvio devolve a mesma resposta"
    );

    let lista: Vec<ItemPessoa> = clientes(&a.app, &t, a.empresa_ana).await.valor();
    assert_eq!(lista.len(), 1, "e não cria a pessoa duas vezes");
    assert_eq!(lista[0].nome, "Maria");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comando_sem_chave_de_idempotencia_e_recusado() {
    let a = ambiente(Duration::from_secs(600));
    let t = token(&a.app, "ana@x.com").await;
    let r = chamar(
        &a.app,
        Method::POST,
        &rota_comando(a.empresa_ana, "clientes.criar_pessoa.v1"),
        Some(&t),
        &[],
        postcard::to_stdvec(&criar_pessoa("X")).unwrap(),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.erro().campo.as_deref(), Some(CABECALHO_IDEMPOTENCIA));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn uma_empresa_nunca_ve_nem_toca_a_outra() {
    let a = ambiente(Duration::from_secs(600));
    let ana = token(&a.app, "ana@x.com").await;
    let beto = token(&a.app, "beto@x.com").await;

    comando(
        &a.app,
        &ana,
        a.empresa_ana,
        "clientes.criar_pessoa.v1",
        &criar_pessoa("Cliente da Ana"),
        Id::novo(),
    )
    .await
    .valor::<mod_clientes::PessoaCadastrada>();

    let do_beto: Vec<ItemPessoa> = clientes(&a.app, &beto, a.empresa_beto).await.valor();
    assert!(do_beto.is_empty(), "base do Beto é outro arquivo");

    let invasao = clientes(&a.app, &beto, a.empresa_ana).await;
    assert_eq!(invasao.status, StatusCode::FORBIDDEN);
    let escrita = comando(
        &a.app,
        &beto,
        a.empresa_ana,
        "clientes.criar_pessoa.v1",
        &criar_pessoa("intruso"),
        Id::novo(),
    )
    .await;
    assert_eq!(escrita.status, StatusCode::FORBIDDEN);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sem_token_sem_protocolo_e_depois_do_logout_e_recusado() {
    let a = ambiente(Duration::from_secs(600));
    let sem_token = chamar(
        &a.app,
        Method::POST,
        &rota_consulta(a.empresa_ana, "clientes.pessoas_por_papel.v1"),
        None,
        &[],
        vec![],
    )
    .await;
    assert_eq!(sem_token.status, StatusCode::UNAUTHORIZED);

    let sem_protocolo = a
        .app
        .clone()
        .oneshot(Request::post(ROTA_SESSAO).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(sem_protocolo.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let t = token(&a.app, "ana@x.com").await;
    assert_eq!(
        clientes(&a.app, &t, a.empresa_ana).await.status,
        StatusCode::OK
    );
    let saida = chamar(&a.app, Method::DELETE, ROTA_SESSAO, Some(&t), &[], vec![]).await;
    assert_eq!(saida.status, StatusCode::NO_CONTENT);
    assert_eq!(
        clientes(&a.app, &t, a.empresa_ana).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empresa_abre_sob_demanda_fecha_ociosa_e_reabre_sem_perder_nada() {
    let a = ambiente(Duration::ZERO);
    assert_eq!(
        a.servidor.frota().abertas(),
        0,
        "boot não abre empresa nenhuma"
    );

    let t = token(&a.app, "ana@x.com").await;
    comando(
        &a.app,
        &t,
        a.empresa_ana,
        "clientes.criar_pessoa.v1",
        &criar_pessoa("Persistente"),
        Id::novo(),
    )
    .await
    .valor::<mod_clientes::PessoaCadastrada>();
    assert_eq!(a.servidor.frota().abertas(), 1);

    let s = Arc::clone(&a.servidor);
    tokio::task::spawn_blocking(move || s.manutencao())
        .await
        .unwrap();
    assert_eq!(
        a.servidor.frota().abertas(),
        0,
        "ociosa = fechada, RAM devolvida"
    );

    let lista: Vec<ItemPessoa> = clientes(&a.app, &t, a.empresa_ana).await.valor();
    assert_eq!(
        lista.len(),
        1,
        "reaberta de forma transparente, com os dados"
    );
    assert_eq!(a.servidor.frota().abertas(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn respostas_da_api_trazem_cabecalhos_de_seguranca_e_nunca_vao_para_cache() {
    let a = ambiente(Duration::from_secs(600));
    let r = login(&a.app, "ana@x.com", SENHA, TipoCliente::Nativo).await;
    let h = &r.cabecalhos;
    assert_eq!(h.get(header::CACHE_CONTROL).unwrap(), "no-store");
    assert_eq!(h.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(), "nosniff");
    assert!(h.get(header::STRICT_TRANSPORT_SECURITY).is_some());
    assert!(h
        .get(header::CONTENT_SECURITY_POLICY)
        .unwrap()
        .to_str()
        .unwrap()
        .contains("default-src 'none'"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn login_demais_do_mesmo_ip_recebe_429_mas_outro_ip_entra() {
    let a = ambiente_com(|c| {
        c.logins_por_ip = 3;
        c.confiar_cloudflare = true;
    });
    let pedido = postcard::to_stdvec(&PedidoLogin {
        email: "ana@x.com".into(),
        senha: "errada-errada".into(),
        cliente: TipoCliente::Nativo,
    })
    .unwrap();
    let de = |ip: &str| vec![("cf-connecting-ip", ip.to_owned())];
    for _ in 0..3 {
        let r = chamar(
            &a.app,
            Method::POST,
            ROTA_SESSAO,
            None,
            &de("203.0.113.7"),
            pedido.clone(),
        )
        .await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    }
    let bloqueado = chamar(
        &a.app,
        Method::POST,
        ROTA_SESSAO,
        None,
        &de("203.0.113.7"),
        pedido.clone(),
    )
    .await;
    assert_eq!(bloqueado.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(bloqueado.erro().codigo, CodigoErro::MUITAS_TENTATIVAS);

    let certo = postcard::to_stdvec(&PedidoLogin {
        email: "beto@x.com".into(),
        senha: SENHA.into(),
        cliente: TipoCliente::Nativo,
    })
    .unwrap();
    let outro = chamar(
        &a.app,
        Method::POST,
        ROTA_SESSAO,
        None,
        &de("198.51.100.4"),
        certo,
    )
    .await;
    assert_eq!(outro.status, StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn despejo_poda_respostas_idempotentes_vencidas() {
    let a = ambiente_com(|c| {
        c.ociosidade = Duration::ZERO;
        c.retencao_idempotencia = Duration::ZERO;
    });
    let t = token(&a.app, "ana@x.com").await;
    let chave = Id::novo();
    let pessoa = |nome| criar_pessoa(nome);
    comando(
        &a.app,
        &t,
        a.empresa_ana,
        "clientes.criar_pessoa.v1",
        &pessoa("Primeira"),
        chave,
    )
    .await
    .valor::<mod_clientes::PessoaCadastrada>();

    let s = Arc::clone(&a.servidor);
    tokio::task::spawn_blocking(move || s.manutencao())
        .await
        .unwrap();

    // Retenção zero: a chave foi podada no despejo, então o mesmo envio executa de novo.
    comando(
        &a.app,
        &t,
        a.empresa_ana,
        "clientes.criar_pessoa.v1",
        &pessoa("Primeira"),
        chave,
    )
    .await
    .valor::<mod_clientes::PessoaCadastrada>();
    let lista: Vec<ItemPessoa> = clientes(&a.app, &t, a.empresa_ana).await.valor();
    assert_eq!(lista.len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dados_e_logo_da_empresa_pelo_despacho() {
    let a = ambiente(Duration::from_secs(600));
    let t = token(&a.app, "ana@x.com").await;
    let consultar = |nome: &'static str| {
        let app = a.app.clone();
        let t = t.clone();
        let empresa = a.empresa_ana;
        async move {
            chamar(
                &app,
                Method::POST,
                &rota_consulta(empresa, nome),
                Some(&t),
                &[],
                postcard::to_stdvec(&()).unwrap(),
            )
            .await
        }
    };
    let dados: mod_empresa::EmpresaResumo = consultar("empresa.dados.v1").await.valor();
    assert_eq!(dados.razao_social, "Loja da Ana");

    let nao_png = mod_empresa::DefinirLogoEmpresa {
        png: Some(b"GIF89a".to_vec()),
    };
    let r = comando(
        &a.app,
        &t,
        a.empresa_ana,
        "empresa.definir_logo.v1",
        &nao_png,
        Id::novo(),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);

    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    png.extend_from_slice(&[0; 32]);
    let logo = mod_empresa::DefinirLogoEmpresa {
        png: Some(png.clone()),
    };
    comando(
        &a.app,
        &t,
        a.empresa_ana,
        "empresa.definir_logo.v1",
        &logo,
        Id::novo(),
    )
    .await
    .valor::<()>();
    let identidade: mod_empresa::IdentidadeVisual =
        consultar("empresa.identidade_visual.v1").await.valor();
    assert_eq!(identidade.logo_png, Some(png));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn backup_copia_so_o_que_mudou_e_a_copia_restaura_com_os_dados() {
    let a = ambiente(Duration::from_secs(600));
    let t = token(&a.app, "ana@x.com").await;
    comando(
        &a.app,
        &t,
        a.empresa_ana,
        "clientes.criar_pessoa.v1",
        &criar_pessoa("Cliente no backup"),
        Id::novo(),
    )
    .await
    .valor::<mod_clientes::PessoaCadastrada>();

    // Com a empresa aberta (escritor vivo): o snapshot é uma leitura do WAL.
    let s = Arc::clone(&a.servidor);
    let r1 = tokio::task::spawn_blocking(move || s.fazer_backup(5))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (r1.copiadas, r1.falhas),
        (3, 0),
        "diretório + duas empresas"
    );

    let s = Arc::clone(&a.servidor);
    let r2 = tokio::task::spawn_blocking(move || s.fazer_backup(5))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (r2.copiadas, r2.sem_mudanca),
        (0, 3),
        "nada mudou, nada copiado"
    );

    comando(
        &a.app,
        &t,
        a.empresa_ana,
        "clientes.criar_pessoa.v1",
        &criar_pessoa("Outro"),
        Id::novo(),
    )
    .await
    .valor::<mod_clientes::PessoaCadastrada>();
    let s = Arc::clone(&a.servidor);
    let r3 = tokio::task::spawn_blocking(move || s.fazer_backup(5))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r3.copiadas, 1, "só a empresa que recebeu a escrita");

    // Restaurar: descomprimir a cópia mais nova e abrir como uma base qualquer.
    let pasta = a
        .servidor
        .config()
        .pasta_backup()
        .join(a.empresa_ana.to_string());
    let mut copias: Vec<_> = std::fs::read_dir(&pasta)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".db.zst"))
        .collect();
    copias.sort();
    assert_eq!(copias.len(), 2);
    let restaurada = pasta.join("restaurada.db");
    let bytes = zstd::decode_all(std::fs::File::open(copias.last().unwrap()).unwrap()).unwrap();
    std::fs::write(&restaurada, bytes).unwrap();
    let c = rusqlite::Connection::open(&restaurada).unwrap();
    let nomes: Vec<String> = c
        .prepare("SELECT nome FROM clientes_pessoa ORDER BY nome")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(nomes, ["Cliente no backup", "Outro"]);
    let integridade: String = c
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(integridade, "ok");
}
