//! O despacho de ponta a ponta contra SQLite real: um módulo de brinquedo (`demo`) com um
//! comando que grava e uma consulta que lê, passando por autorização e transação.

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel, Sessao};
use cardeal_kernel::{CodigoErro, Id, Instante, Resultado};
use cardeal_storage::{
    Armazenamento, ConfigArmazenamento, ConjuntoMigracoes, Migracao, TipoMigracao,
    UnidadeDeTrabalho,
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

use crate::despacho::{Ambiente, Comando, Consulta, Despachante, Modulo, Registro};
use crate::icone::Icone;
use crate::manifesto::{IdModulo, Manifesto};
use crate::permissao::{Permissao, Risco};
use crate::registro::{PedidoAtivacao, RegistroModulos};

// ─── o módulo de brinquedo ─────────────────────────────────────────────────

static DEMO_MANIFESTO: Manifesto = Manifesto {
    id: IdModulo::novo("demo"),
    nome: "Demo",
    versao: (0, 1, 0),
    descricao: "Módulo de teste do despacho.",
    icone: Icone::Config,
    depende_de: &[],
    melhora_com: &[],
    conflita_com: &[],
    submodulos: &[],
    permissoes: &[
        Permissao {
            chave: "demo.nota.criar",
            descricao: "Criar nota",
            risco: Risco::Medio,
            requer_submodulo: None,
        },
        Permissao {
            chave: "demo.nota.ver",
            descricao: "Ver notas",
            risco: Risco::Baixo,
            requer_submodulo: None,
        },
    ],
    menu: &[],
    contas_requeridas: &[],
    eventos_publicados: &[],
    eventos_assinados: &[],
};

const DEMO_SQL: &str = "CREATE TABLE demo_nota (
    id        BLOB PRIMARY KEY,
    empresa   BLOB    NOT NULL,
    texto     TEXT    NOT NULL,
    criado_em INTEGER NOT NULL
) STRICT;";

const DEMO_MIGRACOES: &[Migracao] = &[Migracao {
    versao: 1,
    nome: "demo_inicial",
    sql: DEMO_SQL,
    tipo: TipoMigracao::Esquema,
}];

struct ModuloDemo;

impl Modulo for ModuloDemo {
    fn manifesto(&self) -> &'static Manifesto {
        &DEMO_MANIFESTO
    }

    fn migracoes(&self) -> ConjuntoMigracoes {
        ConjuntoMigracoes {
            modulo: "demo",
            depende_de: &["nucleo"],
            migracoes: DEMO_MIGRACOES,
        }
    }

    fn registrar(&self, registro: &mut Registro) -> Resultado<()> {
        registro
            .comando::<CriarNota>("demo.criar_nota.v1")
            .consulta::<ContarNotas>("demo.contar_notas.v1");
        Ok(())
    }
}

/// Um módulo que colide de propósito com um nome de comando de `demo`.
struct ModuloColidente;

impl Modulo for ModuloColidente {
    fn manifesto(&self) -> &'static Manifesto {
        &DEMO_MANIFESTO
    }

    fn migracoes(&self) -> ConjuntoMigracoes {
        ConjuntoMigracoes {
            modulo: "demo",
            depende_de: &["nucleo"],
            migracoes: DEMO_MIGRACOES,
        }
    }

    fn registrar(&self, registro: &mut Registro) -> Resultado<()> {
        registro.comando::<CriarNota>("demo.criar_nota.v1");
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct CriarNota {
    texto: String,
}

#[derive(Serialize, Deserialize)]
struct NotaCriada {
    id: Vec<u8>,
}

impl Comando for CriarNota {
    type Saida = NotaCriada;
    const PERMISSAO: &'static str = "demo.nota.criar";

    fn executar(self, ctx: &crate::Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<NotaCriada> {
        if self.texto.trim().is_empty() {
            return Err(cardeal_kernel::Erro::novo(
                CodigoErro::CAMPO_OBRIGATORIO,
                "o texto da nota é obrigatório",
            )
            .no_campo("texto"));
        }
        let id = Id::novo();
        uow.conexao()
            .execute(
                "INSERT INTO demo_nota (id, empresa, texto, criado_em) VALUES (?1,?2,?3,?4)",
                params![
                    id.em_bytes().as_slice(),
                    ctx.empresa.em_bytes().as_slice(),
                    self.texto,
                    ctx.agora.em_micros(),
                ],
            )
            .map_err(erro_sql)?;
        Ok(NotaCriada {
            id: id.em_bytes().to_vec(),
        })
    }
}

#[derive(Serialize, Deserialize)]
struct ContarNotas;

impl Consulta for ContarNotas {
    type Saida = i64;
    const PERMISSAO: &'static str = "demo.nota.ver";

    fn executar(self, ctx: &crate::Ctx, conexao: &Connection) -> Resultado<i64> {
        conexao
            .query_row(
                "SELECT COUNT(*) FROM demo_nota WHERE empresa = ?1",
                [ctx.empresa.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(erro_sql)
    }
}

#[allow(clippy::needless_pass_by_value)]
fn erro_sql(e: rusqlite::Error) -> cardeal_kernel::Erro {
    cardeal_kernel::Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())
}

// ─── infraestrutura de teste ───────────────────────────────────────────────

fn ambiente_demo(empresa: Id) -> Ambiente {
    let mut rm = RegistroModulos::novo();
    rm.registrar(&DEMO_MANIFESTO).unwrap();
    let efetivo = rm
        .resolver(&PedidoAtivacao::nova().com_modulo("demo"))
        .unwrap();
    Ambiente::novo(empresa, efetivo)
}

fn ambiente_vazio(empresa: Id) -> Ambiente {
    let efetivo = RegistroModulos::novo()
        .resolver(&PedidoAtivacao::nova())
        .unwrap();
    Ambiente::novo(empresa, efetivo)
}

fn sessao_com(empresa: Id, permissoes: &[&str]) -> Sessao {
    let mut papel = Papel::novo(empresa, "Operador");
    for p in permissoes {
        papel = papel.com_permissao(*p);
    }
    let auth = AutorizacoesEfetivas::consolidar([&papel]);
    Sessao::abrir(EmissaoSessao::padrao(
        Id::novo(),
        Id::novo(),
        Escopo::empresa_inteira(empresa),
        auth,
        Instante::EPOCA,
    ))
}

fn base() -> (TempDir, Armazenamento) {
    let dir = tempfile::tempdir().unwrap();
    let arm =
        Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("cardeal.db"))).unwrap();
    arm.migrar(&[ModuloDemo.migracoes()]).unwrap();
    (dir, arm)
}

fn despachante() -> Despachante {
    Despachante::construir(&[&ModuloDemo]).unwrap()
}

// ─── testes ────────────────────────────────────────────────────────────────

#[test]
fn comando_autorizado_grava_e_a_consulta_le() {
    let (_dir, arm) = base();
    let d = despachante();
    let empresa = Id::novo();
    let ambiente = ambiente_demo(empresa);
    let sessao = sessao_com(empresa, &["demo.nota.criar", "demo.nota.ver"]);

    let carga = postcard::to_stdvec(&CriarNota {
        texto: "primeira nota".into(),
    })
    .unwrap();
    let saida = d
        .executar_comando(
            "demo.criar_nota.v1",
            &carga,
            &sessao,
            &ambiente,
            arm.escritor(),
        )
        .unwrap();
    let nota: NotaCriada = postcard::from_bytes(&saida).unwrap();
    assert_eq!(nota.id.len(), 16);

    let total_bytes = d
        .executar_consulta(
            "demo.contar_notas.v1",
            &postcard::to_stdvec(&ContarNotas).unwrap(),
            &sessao,
            &ambiente,
            arm.leitor(),
        )
        .unwrap();
    let total: i64 = postcard::from_bytes(&total_bytes).unwrap();
    assert_eq!(total, 1);
}

#[test]
fn sem_a_permissao_o_comando_e_recusado_antes_de_tocar_o_banco() {
    let (_dir, arm) = base();
    let d = despachante();
    let empresa = Id::novo();
    let ambiente = ambiente_demo(empresa);
    let sessao = sessao_com(empresa, &["demo.nota.ver"]); // sem "criar"

    let carga = postcard::to_stdvec(&CriarNota { texto: "x".into() }).unwrap();
    let erro = d
        .executar_comando(
            "demo.criar_nota.v1",
            &carga,
            &sessao,
            &ambiente,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::SEM_PERMISSAO);
}

#[test]
fn modulo_inativo_na_empresa_recusa_o_comando() {
    let (_dir, arm) = base();
    let d = despachante();
    let empresa = Id::novo();
    let ambiente = ambiente_vazio(empresa); // "demo" não está ativo
    let sessao = sessao_com(empresa, &["demo.nota.criar"]);

    let carga = postcard::to_stdvec(&CriarNota { texto: "x".into() }).unwrap();
    let erro = d
        .executar_comando(
            "demo.criar_nota.v1",
            &carga,
            &sessao,
            &ambiente,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ESTADO_INVALIDO);
}

#[test]
fn comando_desconhecido_da_nao_encontrado() {
    let (_dir, arm) = base();
    let d = despachante();
    let empresa = Id::novo();
    let ambiente = ambiente_demo(empresa);
    let sessao = sessao_com(empresa, &["demo.nota.criar"]);

    let erro = d
        .executar_comando(
            "demo.inexistente.v1",
            &[],
            &sessao,
            &ambiente,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::NAO_ENCONTRADO);
}

#[test]
fn erro_de_dominio_do_comando_sobe_intacto_e_nada_e_gravado() {
    let (_dir, arm) = base();
    let d = despachante();
    let empresa = Id::novo();
    let ambiente = ambiente_demo(empresa);
    let sessao = sessao_com(empresa, &["demo.nota.criar", "demo.nota.ver"]);

    let carga = postcard::to_stdvec(&CriarNota {
        texto: "   ".into(),
    })
    .unwrap();
    let erro = d
        .executar_comando(
            "demo.criar_nota.v1",
            &carga,
            &sessao,
            &ambiente,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::CAMPO_OBRIGATORIO);
    assert_eq!(erro.campo.as_deref(), Some("texto"));

    // O SAVEPOINT foi desfeito: nenhuma nota gravada.
    let total_bytes = d
        .executar_consulta(
            "demo.contar_notas.v1",
            &postcard::to_stdvec(&ContarNotas).unwrap(),
            &sessao,
            &ambiente,
            arm.leitor(),
        )
        .unwrap();
    let total: i64 = postcard::from_bytes(&total_bytes).unwrap();
    assert_eq!(total, 0);
}

#[test]
fn construir_recusa_dois_modulos_com_o_mesmo_nome_de_comando() {
    let erro = Despachante::construir(&[&ModuloDemo, &ModuloColidente]).unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::DUPLICADO);
}

#[test]
fn contexto_de_escrita_carrega_empresa_usuario_e_dispositivo_da_sessao() {
    let (_dir, arm) = base();
    let d = despachante();
    let empresa = Id::novo();
    let ambiente = ambiente_demo(empresa);
    let sessao = sessao_com(empresa, &["demo.nota.criar", "demo.nota.ver"]);

    // Comando que confere o Ctx contra a sessão e o ambiente.
    let carga = postcard::to_stdvec(&CriarNota {
        texto: "confere ctx".into(),
    })
    .unwrap();
    // Se o Ctx não tivesse a empresa certa, a consulta abaixo (filtrada por empresa) daria 0.
    d.executar_comando(
        "demo.criar_nota.v1",
        &carga,
        &sessao,
        &ambiente,
        arm.escritor(),
    )
    .unwrap();

    let total_bytes = d
        .executar_consulta(
            "demo.contar_notas.v1",
            &postcard::to_stdvec(&ContarNotas).unwrap(),
            &sessao,
            &ambiente,
            arm.leitor(),
        )
        .unwrap();
    assert_eq!(postcard::from_bytes::<i64>(&total_bytes).unwrap(), 1);
}
