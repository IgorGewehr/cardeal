//! Apontamento de tempo por OS, atravessando o `Despachante` real contra SQLite: um técnico
//! não pode ter dois apontamentos abertos ao mesmo tempo (em ordens diferentes), encerrar
//! calcula a duração, ajustar corrige um esquecimento com motivo, e as consultas de tempo
//! total refletem o que foi gravado.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao};
use cardeal_kernel::{CodigoErro, Id, Instante};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_clientes::ModuloClientes;
use mod_os::{
    AbrirOrdemServico, AjustarApontamento, ApontamentoIniciado, ApontamentosDaOrdem,
    EncerrarApontamento, IniciarApontamento, ModuloOs, OrdemServicoAberta,
};
use tempfile::TempDir;

fn base() -> (TempDir, Armazenamento, Id) {
    let dir = tempfile::tempdir().unwrap();
    let arm =
        Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("cardeal.db"))).unwrap();
    arm.migrar(&[
        cardeal_ledger::migracoes::conjunto(),
        ModuloClientes.migracoes(),
        ModuloOs.migracoes(),
    ])
    .unwrap();

    let empresa = Id::novo();
    let ctx = ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx, move |uow| {
            uow.conexao()
                .execute(
                    "INSERT INTO nucleo_empresa
                       (id, razao_social, nome_fantasia, cnpj, regime, endereco, perfil, criado_em)
                     VALUES (?1,'Teste LTDA','Teste','11222333000181','SimplesNacional','{}','assistencia',0)",
                    [empresa.em_bytes().as_slice()],
                )
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
            semear_plano_padrao(uow, empresa)
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap();

    (dir, arm, empresa)
}

fn ambiente(empresa: Id) -> Ambiente {
    let mut rm = RegistroModulos::novo();
    rm.registrar(&mod_os::MANIFESTO).unwrap();
    let efetivo = rm
        .resolver(&PedidoAtivacao::nova().com_modulo("os"))
        .unwrap();
    Ambiente::novo(empresa, efetivo)
}

fn sessao_completa(empresa: Id) -> Sessao {
    let mut papel = PapelAuth::novo(empresa, "Testador");
    for p in [
        "os.ordem.criar",
        "os.apontamento.iniciar",
        "os.apontamento.encerrar",
        "os.apontamento.ajustar",
        "os.ordem.ver",
    ] {
        papel = papel.com_permissao(p);
    }
    Sessao::abrir(EmissaoSessao::padrao(
        Id::novo(),
        Id::novo(),
        Escopo::empresa_inteira(empresa),
        AutorizacoesEfetivas::consolidar([&papel]),
        Instante::EPOCA,
    ))
}

fn carga(v: &impl serde::Serialize) -> Vec<u8> {
    postcard::to_stdvec(v).unwrap()
}

/// Grava uma pessoa direto em `clientes_pessoa` (sem passar por `mod_clientes::CriarPessoa`
/// — este arquivo testa apontamento, não cadastro) só para satisfazer a validação de
/// existência que `AbrirOrdemServico` agora faz.
fn criar_cliente(arm: &Armazenamento, empresa: Id) -> Id {
    let cliente = Id::novo();
    let ctx = ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx, move |uow| {
            uow.conexao()
                .execute(
                    "INSERT INTO clientes_pessoa (id, empresa, tipo, nome, estado, versao, criado_em)
                     VALUES (?1,?2,'Fisica','Cliente Teste','Ativa',1,0)",
                    [cliente.em_bytes().as_slice(), empresa.em_bytes().as_slice()],
                )
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap();
    cliente
}

fn abrir_uma_os(d: &Despachante, s: &Sessao, amb: &Ambiente, arm: &Armazenamento, cliente: Id) -> Id {
    let saida = d
        .executar_comando(
            "os.abrir_ordem_servico.v1",
            &carga(&AbrirOrdemServico {
                cliente,
                equipamento: "Notebook Dell XPS 13".to_string(),
                defeito_relatado: "Não liga".to_string(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            s,
            amb,
            arm.escritor(),
        )
        .unwrap();
    let os: OrdemServicoAberta = postcard::from_bytes(&saida).unwrap();
    os.ordem_servico
}

#[test]
fn tecnico_nao_pode_ter_dois_apontamentos_abertos_ao_mesmo_tempo() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);
    let tecnico = Id::novo();
    let cliente = criar_cliente(&arm, empresa);

    let os1 = abrir_uma_os(&d, &s, &amb, &arm, cliente);
    let os2 = abrir_uma_os(&d, &s, &amb, &arm, cliente);

    // Começa a trabalhar na primeira OS.
    d.executar_comando(
        "os.iniciar_apontamento.v1",
        &carga(&IniciarApontamento {
            ordem_servico: os1,
            tecnico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // Tenta começar a segunda sem encerrar a primeira: recusado.
    let erro = d
        .executar_comando(
            "os.iniciar_apontamento.v1",
            &carga(&IniciarApontamento {
                ordem_servico: os2,
                tecnico,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::REGRA_VIOLADA);

    // Um técnico diferente pode começar normalmente.
    d.executar_comando(
        "os.iniciar_apontamento.v1",
        &carga(&IniciarApontamento {
            ordem_servico: os2,
            tecnico: Id::novo(),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
}

#[test]
fn encerrar_libera_o_tecnico_para_outra_os_e_a_consulta_lista_o_apontamento() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);
    let tecnico = Id::novo();
    let cliente = criar_cliente(&arm, empresa);

    let os1 = abrir_uma_os(&d, &s, &amb, &arm, cliente);
    let os2 = abrir_uma_os(&d, &s, &amb, &arm, cliente);

    let saida = d
        .executar_comando(
            "os.iniciar_apontamento.v1",
            &carga(&IniciarApontamento {
                ordem_servico: os1,
                tecnico,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap();
    let iniciado: ApontamentoIniciado = postcard::from_bytes(&saida).unwrap();

    // Encerra: devolve a duração (>= 0 — o teste roda rápido demais para um segundo
    // completo passar, mas o importante é a transição e a liberação do técnico).
    let saida = d
        .executar_comando(
            "os.encerrar_apontamento.v1",
            &carga(&EncerrarApontamento {
                apontamento: iniciado.apontamento,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap();
    let duracao: i64 = postcard::from_bytes(&saida).unwrap();
    assert!(duracao >= 0);

    // Encerrar de novo é recusado.
    let erro = d
        .executar_comando(
            "os.encerrar_apontamento.v1",
            &carga(&EncerrarApontamento {
                apontamento: iniciado.apontamento,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::REGRA_VIOLADA);

    // Agora o técnico está livre para começar na segunda OS.
    d.executar_comando(
        "os.iniciar_apontamento.v1",
        &carga(&IniciarApontamento {
            ordem_servico: os2,
            tecnico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // A consulta lista o apontamento encerrado da primeira OS.
    let saida = d
        .executar_consulta(
            "os.apontamentos_da_ordem.v1",
            &carga(&ApontamentosDaOrdem { ordem_servico: os1 }),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap();
    let lista: Vec<mod_os::ApontamentoDeTempo> = postcard::from_bytes(&saida).unwrap();
    assert_eq!(lista.len(), 1);
    assert!(!lista[0].esta_aberto());
    assert!(!lista[0].ajustado);
}

#[test]
fn ajustar_corrige_um_esquecimento_e_marca_ajustado() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);
    let tecnico = Id::novo();
    let cliente = criar_cliente(&arm, empresa);
    let os1 = abrir_uma_os(&d, &s, &amb, &arm, cliente);

    let saida = d
        .executar_comando(
            "os.iniciar_apontamento.v1",
            &carga(&IniciarApontamento {
                ordem_servico: os1,
                tecnico,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap();
    let iniciado: ApontamentoIniciado = postcard::from_bytes(&saida).unwrap();

    // O técnico esqueceu de encerrar — corrige manualmente para 2 horas de trabalho.
    let novo_inicio = Instante::EPOCA;
    let novo_fim = Instante::EPOCA.mais_segundos(7200);
    let saida = d
        .executar_comando(
            "os.ajustar_apontamento.v1",
            &carga(&AjustarApontamento {
                apontamento: iniciado.apontamento,
                novo_inicio,
                novo_fim,
                motivo: "Esqueceu de encerrar o cronômetro ao sair".to_string(),
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap();
    let duracao: i64 = postcard::from_bytes(&saida).unwrap();
    assert_eq!(duracao, 7200);

    // Sem motivo é recusado.
    let saida = d.executar_comando(
        "os.ajustar_apontamento.v1",
        &carga(&AjustarApontamento {
            apontamento: iniciado.apontamento,
            novo_inicio,
            novo_fim,
            motivo: "   ".to_string(),
        }),
        &s,
        &amb,
        arm.escritor(),
    );
    assert!(saida.is_err());

    let saida = d
        .executar_consulta(
            "os.tempo_total_da_ordem.v1",
            &carga(&mod_os::TempoTotalDaOrdem { ordem_servico: os1 }),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap();
    let total: i64 = postcard::from_bytes(&saida).unwrap();
    assert_eq!(total, 7200);

    let saida = d
        .executar_consulta(
            "os.apontamentos_da_ordem.v1",
            &carga(&ApontamentosDaOrdem { ordem_servico: os1 }),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap();
    let lista: Vec<mod_os::ApontamentoDeTempo> = postcard::from_bytes(&saida).unwrap();
    assert_eq!(lista.len(), 1);
    assert!(lista[0].ajustado);
    assert_eq!(
        lista[0].motivo_ajuste.as_deref(),
        Some("Esqueceu de encerrar o cronômetro ao sair")
    );
}
