//! O cliente remoto contra um `cardeal-server` de verdade, escutando em TCP: login, escolha de
//! empresa, comando, consulta, a fachada `Motor`, e a retentativa idempotente quando a rede
//! perde a resposta de um comando.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};

use cardeal_cliente::remoto::{Pedido, Remoto, Resposta, Transporte, TransporteHttp};
use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::CodigoErro;
use cardeal_server::provisionamento::{provisionar, NovaEmpresa};
use cardeal_server::{roteador, ConfigServidor, Servidor};
use mod_clientes::{CriarPessoa, ItemPessoa, Papel, PessoasPorPapel, TipoPessoa};

const SENHA: &str = "senha-forte-123";

/// Sobe um servidor numa porta livre, numa thread com o próprio runtime.
fn servidor() -> (tempfile::TempDir, String) {
    let pasta = tempfile::tempdir().unwrap();
    let servidor = Servidor::abrir(ConfigServidor::em(pasta.path())).unwrap();
    for (empresa, email) in [("Loja da Ana", "ana@x.com"), ("Outra Loja", "ana@x.com")] {
        provisionar(
            servidor.config(),
            servidor.plano(),
            servidor.diretorio(),
            &NovaEmpresa {
                razao_social: empresa.into(),
                cnpj: "11222333000181".into(),
                email: email.into(),
                nome: "Ana".into(),
                senha: SENHA.into(),
            },
        )
        .unwrap();
    }
    let ouvinte = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endereco = ouvinte.local_addr().unwrap();
    ouvinte.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let ouvinte = tokio::net::TcpListener::from_std(ouvinte).unwrap();
                axum::serve(
                    ouvinte,
                    roteador(servidor).into_make_service_with_connect_info::<SocketAddr>(),
                )
                .await
                .unwrap();
            });
    });
    (pasta, format!("http://{endereco}"))
}

fn pessoa(nome: &str) -> CriarPessoa {
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

fn clientes(r: &Remoto) -> Vec<ItemPessoa> {
    r.consultar(
        "clientes.pessoas_por_papel.v1",
        &PessoasPorPapel {
            papel: Papel::Cliente,
            busca: None,
        },
    )
    .unwrap()
}

#[test]
fn login_empresa_comando_e_consulta_pela_rede() {
    let (_p, base) = servidor();
    let transporte = Box::new(TransporteHttp::novo(&base).unwrap());
    let mut r = Remoto::entrar(transporte, "ana@x.com", SENHA).unwrap();
    assert_eq!(r.nome(), "Ana");
    assert_eq!(r.empresas().len(), 2);
    assert!(
        r.empresa().is_none(),
        "duas empresas: a escolha é da pessoa"
    );

    let erro = r
        .consultar("empresa.dados.v1", &mod_empresa::DadosDaEmpresa)
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ESTADO_INVALIDO);

    let empresa = r
        .empresas()
        .iter()
        .find(|e| e.nome == "Loja da Ana")
        .unwrap()
        .id;
    let sessao = r.escolher_empresa(empresa).unwrap();
    assert!(
        sessao.concede("clientes.pessoa.criar"),
        "admin recebe o catálogo inteiro"
    );

    r.executar("clientes.criar_pessoa.v1", &pessoa("Maria"))
        .unwrap();
    assert_eq!(clientes(&r)[0].nome, "Maria");

    // A fachada que as telas usam, sobre o mesmo cliente remoto.
    let motor = Motor::Remoto(r);
    let sessao = Sessao::Remota(sessao);
    assert!(motor.e_remoto());
    assert_eq!(
        motor.identidade_visual().unwrap().razao_social,
        "Loja da Ana"
    );
    let lista: Vec<ItemPessoa> = motor
        .consultar(
            &sessao,
            "clientes.pessoas_por_papel.v1",
            &PessoasPorPapel {
                papel: Papel::Cliente,
                busca: None,
            },
        )
        .unwrap();
    assert_eq!(lista.len(), 1);
}

#[test]
fn senha_errada_e_servidor_fora_do_ar_dao_erros_distintos() {
    let (_p, base) = servidor();
    let erro = Remoto::entrar(
        Box::new(TransporteHttp::novo(&base).unwrap()),
        "ana@x.com",
        "x",
    )
    .err()
    .unwrap();
    assert_eq!(erro.codigo, CodigoErro::CREDENCIAL_INVALIDA);

    let fora = TransporteHttp::novo("http://127.0.0.1:9").unwrap();
    let erro = Remoto::entrar(Box::new(fora), "ana@x.com", SENHA)
        .err()
        .unwrap();
    assert_eq!(erro.codigo, CodigoErro::SEM_CONEXAO);
}

/// Entrega o pedido ao servidor de verdade, mas "perde" a resposta das primeiras `perdas`
/// chamadas de comando — o pior caso da rede: o servidor fez, o cliente não ficou sabendo.
struct RedeQuePerdeResposta {
    real: TransporteHttp,
    perdas: AtomicU32,
}

impl Transporte for RedeQuePerdeResposta {
    fn enviar(&self, pedido: &Pedido, token: Option<&str>) -> Result<Resposta, String> {
        let resposta = self.real.enviar(pedido, token)?;
        if pedido.caminho.contains("/cmd/") && self.perdas.load(Ordering::SeqCst) > 0 {
            self.perdas.fetch_sub(1, Ordering::SeqCst);
            return Err("conexão caiu antes da resposta".into());
        }
        Ok(resposta)
    }
}

#[test]
fn resposta_perdida_e_reenviada_com_a_mesma_chave_sem_duplicar() {
    let (_p, base) = servidor();
    let transporte = RedeQuePerdeResposta {
        real: TransporteHttp::novo(&base).unwrap(),
        perdas: AtomicU32::new(2),
    };
    let mut r = Remoto::entrar(Box::new(transporte), "ana@x.com", SENHA).unwrap();
    let empresa = r.empresas()[0].id;
    r.escolher_empresa(empresa).unwrap();

    // Duas respostas perdidas, terceira tentativa volta: o servidor executou três vezes o
    // mesmo pedido, mas a chave de idempotência garante um único cadastro.
    r.executar("clientes.criar_pessoa.v1", &pessoa("Uma vez só"))
        .unwrap();
    assert_eq!(clientes(&r).len(), 1);
}

#[test]
fn desktop_acompanha_em_tempo_real_o_que_outra_sessao_faz() {
    use cardeal_cliente::remoto::AvisoTempoReal;
    use std::time::{Duration, Instant};

    let (_p, base) = servidor();
    let entrar = || {
        let mut r = Remoto::entrar(
            Box::new(TransporteHttp::novo(&base).unwrap()),
            "ana@x.com",
            SENHA,
        )
        .unwrap();
        let empresa = r
            .empresas()
            .iter()
            .find(|e| e.nome == "Loja da Ana")
            .unwrap()
            .id;
        r.escolher_empresa(empresa).unwrap();
        r
    };
    let caixa = entrar();
    let balcao = entrar();

    let acordou = std::sync::Arc::new(AtomicU32::new(0));
    let a = std::sync::Arc::clone(&acordou);
    let assinatura = caixa
        .acompanhar(move || {
            a.fetch_add(1, Ordering::Relaxed);
        })
        .unwrap();
    // Dá tempo de o fluxo conectar antes do comando (o que viesse antes seria só "pronto").
    std::thread::sleep(Duration::from_millis(300));

    let inicio = Instant::now();
    balcao
        .executar("clientes.criar_pessoa.v1", &pessoa("Gabi"))
        .unwrap();
    let aviso = loop {
        if let Some(a) = assinatura.drenar().into_iter().next() {
            break a;
        }
        assert!(
            inicio.elapsed() < Duration::from_secs(5),
            "o aviso não chegou"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(aviso.afeta("clientes"), "{aviso:?}");
    assert_ne!(aviso, AvisoTempoReal::SessaoEncerrada);
    assert!(
        acordou.load(Ordering::Relaxed) >= 1,
        "a interface é acordada"
    );
    println!("aviso em {:?}", inicio.elapsed());

    // O logout derruba o fluxo no próximo batimento; aqui basta soltar a assinatura.
    drop(assinatura);
}
