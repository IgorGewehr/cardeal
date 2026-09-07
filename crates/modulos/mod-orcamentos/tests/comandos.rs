//! O ciclo de um orçamento comercial atravessando o `Despachante` real contra SQLite:
//! criar → definir itens → enviar → aprovar → consultar → converter numa OS de verdade.

#![allow(clippy::result_large_err)]

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao};
use cardeal_kernel::{Id, Instante, Percentual, Preco, Quantidade};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_clientes::{CriarPessoa, ModuloClientes, Papel, PessoaCadastrada, TipoPessoa};
use mod_orcamentos::{
    BuscarOrcamento, ConverterOrcamentoEmOs, CriarOrcamento, DefinirItensOrcamento,
    DetalheOrcamento, EnviarOrcamento, EstadoOrcamento, ModuloOrcamentos, NovoItemOrcamento,
    OrcamentoConvertido, OrcamentoCriado, OrcamentosRecentes, RegistrarDecisaoOrcamento,
    ResumoOrcamentos, ResumoOrcamentosSaida,
};
use mod_os::ModuloOs;
use tempfile::TempDir;

fn base() -> (TempDir, Armazenamento, Id) {
    let dir = tempfile::tempdir().unwrap();
    let arm =
        Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("cardeal.db"))).unwrap();
    arm.migrar(&[
        cardeal_ledger::migracoes::conjunto(),
        ModuloClientes.migracoes(),
        mod_estoque::ModuloEstoque.migracoes(),
        mod_financeiro::ModuloFinanceiro.migracoes(),
        ModuloOs.migracoes(),
        ModuloOrcamentos.migracoes(),
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
                     VALUES (?1,'Teste LTDA','Teste','11222333000181','SimplesNacional','{}','servico',0)",
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
    rm.registrar(&mod_clientes::MANIFESTO).unwrap();
    rm.registrar(&mod_os::MANIFESTO).unwrap();
    rm.registrar(&mod_orcamentos::MANIFESTO).unwrap();
    let efetivo = rm
        .resolver(
            &PedidoAtivacao::nova()
                .com_modulo("clientes")
                .com_modulo("os")
                .com_modulo("orcamentos"),
        )
        .unwrap();
    Ambiente::novo(empresa, efetivo)
}

fn sessao(empresa: Id) -> Sessao {
    let mut papel = PapelAuth::novo(empresa, "Testador");
    for p in [
        "clientes.pessoa.criar",
        "orcamentos.orcamento.ver",
        "orcamentos.orcamento.criar",
        "orcamentos.orcamento.editar",
        "orcamentos.orcamento.enviar",
        "orcamentos.orcamento.decidir",
        "orcamentos.orcamento.cancelar",
        "orcamentos.orcamento.converter",
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

fn item(desc: &str, qtd: i64, preco: i64) -> NovoItemOrcamento {
    NovoItemOrcamento {
        descricao: desc.to_owned(),
        quantidade: Quantidade::unidades(qtd),
        unidade: "un".to_owned(),
        preco_unitario: Preco::reais(preco),
        desconto_percentual: Percentual::ZERO,
    }
}

#[test]
fn ciclo_completo_cria_envia_aprova_e_converte_em_os() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloOs, &ModuloOrcamentos]).unwrap();
    let s = sessao(empresa);
    let amb = ambiente(empresa);

    let cliente: PessoaCadastrada = postcard::from_bytes(
        &d.executar_comando(
            "clientes.criar_pessoa.v1",
            &carga(&CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome: "João Silva".to_owned(),
                nome_fantasia: None,
                papel_inicial: Papel::Cliente,
                documento_tipo: None,
                documento_numero: None,
                data_nascimento: None,
                endereco: None,
                contato: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    // Cria já com um item.
    let criado: OrcamentoCriado = postcard::from_bytes(
        &d.executar_comando(
            "orcamentos.criar_orcamento.v1",
            &carga(&CriarOrcamento {
                cliente: Some(cliente.pessoa),
                cliente_nome: "João Silva".to_owned(),
                cliente_documento: None,
                cliente_contato: Some("(31) 99999-0000".to_owned()),
                assunto: "Rebobinamento de motor".to_owned(),
                descricao: Some("Motor 10cv trifásico".to_owned()),
                validade_dias: 15,
                condicoes_pagamento: Some("50/50".to_owned()),
                prazo_entrega: Some("5 dias".to_owned()),
                observacoes: None,
                desconto_percentual: Percentual::ZERO,
                itens: vec![item("Diagnóstico", 1, 120)],
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(criado.numero, 1);

    // Redefine os itens.
    d.executar_comando(
        "orcamentos.definir_itens.v1",
        &carga(&DefinirItensOrcamento {
            orcamento: criado.orcamento,
            itens: vec![
                item("Rebobinamento", 1, 800),
                item("Rolamentos", 2, 60),
                item("Mão de obra de montagem", 3, 90),
            ],
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // Envia.
    d.executar_comando(
        "orcamentos.enviar_orcamento.v1",
        &carga(&EnviarOrcamento { orcamento: criado.orcamento }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // Consulta a lista filtrando por estado.
    let lista: Vec<mod_orcamentos::ItemOrcamentoLista> = postcard::from_bytes(
        &d.executar_consulta(
            "orcamentos.orcamentos_recentes.v1",
            &carga(&OrcamentosRecentes {
                estado: Some(EstadoOrcamento::Enviado),
                ..Default::default()
            }),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(lista.len(), 1);
    assert_eq!(lista[0].numero, 1);
    assert_eq!(lista[0].total, cardeal_kernel::Dinheiro::reais(1190)); // 800 + 120 + 270
    assert_eq!(lista[0].itens, 3);

    // Busca detalhe.
    let detalhe: DetalheOrcamento = postcard::from_bytes::<Option<DetalheOrcamento>>(
        &d.executar_consulta(
            "orcamentos.buscar_orcamento.v1",
            &carga(&BuscarOrcamento { orcamento: criado.orcamento }),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(detalhe.itens.len(), 3);
    assert_eq!(detalhe.total, cardeal_kernel::Dinheiro::reais(1190));

    // Aprova (exige identificação).
    let erro = d
        .executar_comando(
            "orcamentos.registrar_decisao.v1",
            &carga(&RegistrarDecisaoOrcamento {
                orcamento: criado.orcamento,
                aprovado: true,
                identificacao: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, cardeal_kernel::CodigoErro::CAMPO_OBRIGATORIO);

    d.executar_comando(
        "orcamentos.registrar_decisao.v1",
        &carga(&RegistrarDecisaoOrcamento {
            orcamento: criado.orcamento,
            aprovado: true,
            identificacao: Some("João Silva - CPF 529.982.247-25".to_owned()),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // Converte em OS.
    let convertido: OrcamentoConvertido = postcard::from_bytes(
        &d.executar_comando(
            "orcamentos.converter_em_os.v1",
            &carga(&ConverterOrcamentoEmOs {
                orcamento: criado.orcamento,
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(convertido.numero, 1);

    // A OS existe, com o assunto como equipamento e o valor total dos itens.
    let os_total: i64 = arm
        .leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT valor_total FROM os_ordem_servico WHERE id = ?1",
                [convertido.ordem_servico.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap();
    assert_eq!(os_total, 119_000); // R$ 1.190,00 em centavos

    // O orçamento agora está Convertido.
    let detalhe: DetalheOrcamento = postcard::from_bytes::<Option<DetalheOrcamento>>(
        &d.executar_consulta(
            "orcamentos.buscar_orcamento.v1",
            &carga(&BuscarOrcamento { orcamento: criado.orcamento }),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(detalhe.orcamento.estado, EstadoOrcamento::Convertido);
    assert_eq!(detalhe.orcamento.os_gerada, Some(convertido.ordem_servico));
}

#[test]
fn resumo_conta_abertos_e_conversao() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloOs, &ModuloOrcamentos]).unwrap();
    let s = sessao(empresa);
    let amb = ambiente(empresa);

    let novo = |assunto: &str| -> OrcamentoCriado {
        postcard::from_bytes(
            &d.executar_comando(
                "orcamentos.criar_orcamento.v1",
                &carga(&CriarOrcamento {
                    cliente: None,
                    cliente_nome: "Avulso".to_owned(),
                    cliente_documento: None,
                    cliente_contato: None,
                    assunto: assunto.to_owned(),
                    descricao: None,
                    validade_dias: 30,
                    condicoes_pagamento: None,
                    prazo_entrega: None,
                    observacoes: None,
                    desconto_percentual: Percentual::ZERO,
                    itens: vec![item("Serviço", 1, 500)],
                }),
                &s,
                &amb,
                arm.escritor(),
            )
            .unwrap(),
        )
        .unwrap()
    };

    let a = novo("A");
    let b = novo("B");
    let _c = novo("C"); // fica em rascunho

    for o in [&a, &b] {
        d.executar_comando(
            "orcamentos.enviar_orcamento.v1",
            &carga(&EnviarOrcamento { orcamento: o.orcamento }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap();
    }
    // aprova A, recusa B
    d.executar_comando(
        "orcamentos.registrar_decisao.v1",
        &carga(&RegistrarDecisaoOrcamento {
            orcamento: a.orcamento,
            aprovado: true,
            identificacao: Some("cliente".to_owned()),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    d.executar_comando(
        "orcamentos.registrar_decisao.v1",
        &carga(&RegistrarDecisaoOrcamento {
            orcamento: b.orcamento,
            aprovado: false,
            identificacao: None,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    let resumo: ResumoOrcamentosSaida = postcard::from_bytes(
        &d.executar_consulta(
            "orcamentos.resumo.v1",
            &carga(&ResumoOrcamentos),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(resumo.abertos, 1); // só o C
    assert_eq!(resumo.aprovados_periodo, 1);
    assert_eq!(resumo.taxa_conversao, Percentual::pontos(50)); // 1 de 2 decididos
}
