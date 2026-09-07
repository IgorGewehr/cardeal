//! Os comandos de agenda atravessando o `Despachante` real contra SQLite: autorização,
//! transação e persistência — tudo junto, incluindo a interligação real com `clientes`
//! (`agenda_compromisso.cliente` é uma FK de verdade para `clientes_pessoa`).

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao};
use cardeal_kernel::{CodigoErro, Data, Fuso, Hora, Id, Instante};
use cardeal_modkit::{Ambiente, Ctx, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_agenda::{
    registrar_nao_comparecimento_pendentes, AgendaDoRecurso, CancelarCompromisso, Compromisso,
    CompromissoAgendado, ConfirmarCompromisso, ConflitosDeAgenda, CriarCompromisso, CriarRecurso,
    DefinirDisponibilidade, DisponibilidadeDefinida, DisponibilidadeNoPeriodo, IniciarCompromisso,
    ModuloAgenda, ProximosCompromissos, RecursoCriado, TipoCompromisso, TipoRecurso, MANIFESTO,
};
use mod_clientes::{
    CriarPessoa, ModuloClientes, Papel, PessoaCadastrada, TipoDocumento, TipoPessoa,
};
use tempfile::TempDir;

const FUSO: Fuso = Fuso::BRASILIA;

fn base() -> (TempDir, Armazenamento, Id) {
    let dir = tempfile::tempdir().unwrap();
    let arm =
        Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("cardeal.db"))).unwrap();
    arm.migrar(&[ModuloClientes.migracoes(), ModuloAgenda.migracoes()])
        .unwrap();

    let empresa = Id::novo();
    let ctx = ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx, move |uow| {
            uow.conexao()
                .execute(
                    "INSERT INTO nucleo_empresa
                       (id, razao_social, nome_fantasia, cnpj, regime, endereco, perfil, criado_em)
                     VALUES (?1,'Teste LTDA','Teste','11222333000181','SimplesNacional','{}','comercio',0)",
                    [empresa.em_bytes().as_slice()],
                )
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap();

    (dir, arm, empresa)
}

fn ambiente(empresa: Id) -> Ambiente {
    let mut rm = RegistroModulos::novo();
    rm.registrar(&mod_clientes::MANIFESTO).unwrap();
    rm.registrar(&MANIFESTO).unwrap();
    let efetivo = rm
        .resolver(
            &PedidoAtivacao::nova()
                .com_modulo("clientes")
                .com_modulo("agenda"),
        )
        .unwrap();
    Ambiente::novo(empresa, efetivo)
}

fn sessao(empresa: Id, permissoes: &[&str]) -> Sessao {
    let mut papel = PapelAuth::novo(empresa, "Testador");
    for p in permissoes {
        papel = papel.com_permissao(*p);
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

/// Um `Ctx` de verdade, montado sem passar pelo lookup por nome do `Despachante` — o que
/// `registrar_nao_comparecimento_pendentes` exige (não é `Comando`).
fn ctx_direto(empresa: Id) -> Ctx {
    Ctx::de_sessao(&sessao(empresa, &[]), &ambiente(empresa))
}

/// Datas em 2030 — bem no futuro, para `ProximosCompromissos` (`fim > agora`) sempre pegar.
fn instante(dia: u32, hora: u32) -> Instante {
    Instante::de_data_hora(
        Data::de_ymd(2030, 3, dia).unwrap(),
        Hora::de_hms(hora, 0, 0).unwrap(),
        FUSO,
    )
}

/// Data em 2020 — bem no passado, para `registrar_nao_comparecimento_pendentes` (`fim <=
/// agora`) sempre pegar.
fn instante_passado(hora: u32) -> Instante {
    Instante::de_data_hora(
        Data::de_ymd(2020, 1, 1).unwrap(),
        Hora::de_hms(hora, 0, 0).unwrap(),
        FUSO,
    )
}

fn criar_cliente(d: &Despachante, arm: &Armazenamento, empresa: Id, s: &Sessao) -> Id {
    let saida: PessoaCadastrada = postcard::from_bytes(
        &d.executar_comando(
            "clientes.criar_pessoa.v1",
            &carga(&CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome: "João Silva".to_string(),
                nome_fantasia: None,
                papel_inicial: Papel::Cliente,
                documento_tipo: Some(TipoDocumento::Cpf),
                documento_numero: Some("52998224725".to_string()),
                data_nascimento: None,
                endereco: None,
                contato: None,
            }),
            s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    saida.pessoa
}

fn criar_recurso(d: &Despachante, arm: &Armazenamento, empresa: Id, s: &Sessao, nome: &str) -> Id {
    let saida: RecursoCriado = postcard::from_bytes(
        &d.executar_comando(
            "agenda.criar_recurso.v1",
            &carga(&CriarRecurso {
                nome: nome.to_string(),
                tipo: TipoRecurso::Tecnico,
                capacidade: None,
            }),
            s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    saida.recurso
}

#[test]
fn criar_compromisso_com_cliente_de_verdade_e_recuperado_nos_proximos() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloAgenda]).unwrap();
    let s = sessao(
        empresa,
        &[
            "clientes.pessoa.criar",
            "agenda.recurso.gerenciar",
            "agenda.compromisso.criar",
            "agenda.compromisso.ver",
        ],
    );
    let cliente = criar_cliente(&d, &arm, empresa, &s);
    let tecnico = criar_recurso(&d, &arm, empresa, &s, "Carlos");

    let agendado: CompromissoAgendado = postcard::from_bytes(
        &d.executar_comando(
            "agenda.criar_compromisso.v1",
            &carga(&CriarCompromisso {
                titulo: "Visita técnica".to_string(),
                tipo: TipoCompromisso::Cliente,
                cliente: Some(cliente),
                inicio: instante(10, 9),
                fim: instante(10, 10),
                recursos: vec![tecnico],
                origem_modulo: None,
                origem_id: None,
                fora_do_horario_padrao: false,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let proximos: Vec<Compromisso> = postcard::from_bytes(
        &d.executar_consulta(
            "agenda.proximos_compromissos.v1",
            &carga(&ProximosCompromissos),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(proximos.len(), 1);
    assert_eq!(proximos[0].id, agendado.compromisso);
    assert_eq!(proximos[0].cliente, Some(cliente));
}

#[test]
fn segundo_compromisso_no_mesmo_recurso_e_horario_e_recusado() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloAgenda]).unwrap();
    let s = sessao(
        empresa,
        &["agenda.recurso.gerenciar", "agenda.compromisso.criar"],
    );
    let sala = criar_recurso(&d, &arm, empresa, &s, "Sala 1");

    d.executar_comando(
        "agenda.criar_compromisso.v1",
        &carga(&CriarCompromisso {
            titulo: "Reunião A".to_string(),
            tipo: TipoCompromisso::Interno,
            cliente: None,
            inicio: instante(10, 9),
            fim: instante(10, 10),
            recursos: vec![sala],
            origem_modulo: None,
            origem_id: None,
            fora_do_horario_padrao: false,
        }),
        &s,
        &ambiente(empresa),
        arm.escritor(),
    )
    .unwrap();

    let erro = d
        .executar_comando(
            "agenda.criar_compromisso.v1",
            &carga(&CriarCompromisso {
                titulo: "Reunião B".to_string(),
                tipo: TipoCompromisso::Interno,
                cliente: None,
                inicio: instante(10, 9),
                fim: Instante::de_data_hora(
                    Data::de_ymd(2030, 3, 10).unwrap(),
                    Hora::de_hms(9, 30, 0).unwrap(),
                    FUSO,
                ),
                recursos: vec![sala],
                origem_modulo: None,
                origem_id: None,
                fora_do_horario_padrao: false,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::REGRA_VIOLADA);
}

#[test]
fn fora_da_disponibilidade_e_recusado_sem_confirmacao_explicita() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloAgenda]).unwrap();
    let s = sessao(
        empresa,
        &["agenda.recurso.gerenciar", "agenda.compromisso.criar"],
    );
    let tecnico = criar_recurso(&d, &arm, empresa, &s, "Ana");
    let dia_semana = Data::de_ymd(2030, 3, 10).unwrap().dia_da_semana() as u8;

    // Disponível no dia do teste, das 08:00 às 18:00 (480–1080 minutos).
    let _: DisponibilidadeDefinida = postcard::from_bytes(
        &d.executar_comando(
            "agenda.definir_disponibilidade.v1",
            &carga(&DefinirDisponibilidade {
                recurso: tecnico,
                dia_semana,
                hora_inicio: 480,
                hora_fim: 1080,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    // 20h está fora da janela 08–18h cadastrada acima.
    let erro = d
        .executar_comando(
            "agenda.criar_compromisso.v1",
            &carga(&CriarCompromisso {
                titulo: "Visita noturna".to_string(),
                tipo: TipoCompromisso::Interno,
                cliente: None,
                inicio: instante(10, 20),
                fim: instante(10, 21),
                recursos: vec![tecnico],
                origem_modulo: None,
                origem_id: None,
                fora_do_horario_padrao: false,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::REGRA_VIOLADA);

    // Com a confirmação explícita, é aceito.
    d.executar_comando(
        "agenda.criar_compromisso.v1",
        &carga(&CriarCompromisso {
            titulo: "Visita noturna urgente".to_string(),
            tipo: TipoCompromisso::Interno,
            cliente: None,
            inicio: instante(10, 20),
            fim: instante(10, 21),
            recursos: vec![tecnico],
            origem_modulo: None,
            origem_id: None,
            fora_do_horario_padrao: true,
        }),
        &s,
        &ambiente(empresa),
        arm.escritor(),
    )
    .unwrap();
}

#[test]
fn ciclo_completo_confirmar_iniciar_concluir() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloAgenda]).unwrap();
    let s = sessao(
        empresa,
        &[
            "agenda.recurso.gerenciar",
            "agenda.compromisso.criar",
            "agenda.compromisso.confirmar",
            "agenda.compromisso.cancelar",
        ],
    );
    let sala = criar_recurso(&d, &arm, empresa, &s, "Sala 2");
    let agendado: CompromissoAgendado = postcard::from_bytes(
        &d.executar_comando(
            "agenda.criar_compromisso.v1",
            &carga(&CriarCompromisso {
                titulo: "Manutenção".to_string(),
                tipo: TipoCompromisso::Interno,
                cliente: None,
                inicio: instante(10, 9),
                fim: instante(10, 10),
                recursos: vec![sala],
                origem_modulo: None,
                origem_id: None,
                fora_do_horario_padrao: false,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    d.executar_comando(
        "agenda.confirmar_compromisso.v1",
        &carga(&ConfirmarCompromisso {
            compromisso: agendado.compromisso,
        }),
        &s,
        &ambiente(empresa),
        arm.escritor(),
    )
    .unwrap();
    d.executar_comando(
        "agenda.iniciar_compromisso.v1",
        &carga(&IniciarCompromisso {
            compromisso: agendado.compromisso,
        }),
        &s,
        &ambiente(empresa),
        arm.escritor(),
    )
    .unwrap();

    // Cancelar depois de EmAndamento ainda é permitido; concluir também seria.
    d.executar_comando(
        "agenda.cancelar_compromisso.v1",
        &carga(&CancelarCompromisso {
            compromisso: agendado.compromisso,
        }),
        &s,
        &ambiente(empresa),
        arm.escritor(),
    )
    .unwrap();
}

#[test]
fn consultas_de_recurso_refletem_o_compromisso_criado() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloAgenda]).unwrap();
    let s = sessao(
        empresa,
        &[
            "agenda.recurso.gerenciar",
            "agenda.compromisso.criar",
            "agenda.compromisso.ver",
        ],
    );
    let sala = criar_recurso(&d, &arm, empresa, &s, "Sala 3");
    d.executar_comando(
        "agenda.criar_compromisso.v1",
        &carga(&CriarCompromisso {
            titulo: "Alinhamento".to_string(),
            tipo: TipoCompromisso::Interno,
            cliente: None,
            inicio: instante(10, 9),
            fim: instante(10, 10),
            recursos: vec![sala],
            origem_modulo: None,
            origem_id: None,
            fora_do_horario_padrao: false,
        }),
        &s,
        &ambiente(empresa),
        arm.escritor(),
    )
    .unwrap();

    let agenda: Vec<Compromisso> = postcard::from_bytes(
        &d.executar_consulta(
            "agenda.agenda_do_recurso.v1",
            &carga(&AgendaDoRecurso {
                recurso: sala,
                inicio: instante(10, 0),
                fim: instante(11, 0),
            }),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(agenda.len(), 1);

    let conflitos: Vec<Compromisso> = postcard::from_bytes(
        &d.executar_consulta(
            "agenda.conflitos_de_agenda.v1",
            &carga(&ConflitosDeAgenda {
                recurso: sala,
                inicio: instante(10, 9),
                fim: instante(10, 10),
            }),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(conflitos.len(), 1);

    let disponibilidade: Vec<mod_agenda::DisponibilidadeRecurso> = postcard::from_bytes(
        &d.executar_consulta(
            "agenda.disponibilidade_no_periodo.v1",
            &carga(&DisponibilidadeNoPeriodo { recurso: sala }),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(disponibilidade.is_empty()); // sem regra = sempre disponível
}

#[test]
fn registrar_nao_comparecimento_marca_confirmados_vencidos() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloAgenda]).unwrap();
    let s = sessao(
        empresa,
        &[
            "agenda.recurso.gerenciar",
            "agenda.compromisso.criar",
            "agenda.compromisso.confirmar",
        ],
    );
    let sala = criar_recurso(&d, &arm, empresa, &s, "Sala 4");
    let agendado: CompromissoAgendado = postcard::from_bytes(
        &d.executar_comando(
            "agenda.criar_compromisso.v1",
            &carga(&CriarCompromisso {
                titulo: "Compromisso passado".to_string(),
                tipo: TipoCompromisso::Interno,
                cliente: None,
                inicio: instante_passado(9),
                fim: instante_passado(10),
                recursos: vec![sala],
                origem_modulo: None,
                origem_id: None,
                fora_do_horario_padrao: false,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    d.executar_comando(
        "agenda.confirmar_compromisso.v1",
        &carga(&ConfirmarCompromisso {
            compromisso: agendado.compromisso,
        }),
        &s,
        &ambiente(empresa),
        arm.escritor(),
    )
    .unwrap();

    let ctx = ctx_direto(empresa);
    let marcados: Vec<Id> = arm
        .escritor()
        .executar(
            ContextoEscrita::novo(empresa, ctx.usuario, ctx.dispositivo, Id::novo()),
            move |uow| {
                registrar_nao_comparecimento_pendentes(&ctx, uow)
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            },
        )
        .unwrap()
        .valor();
    assert_eq!(marcados, vec![agendado.compromisso]);
}
