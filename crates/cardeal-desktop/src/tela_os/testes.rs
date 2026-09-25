//! Testes ponta a ponta da tela de OS: motor real (SQLite temporário), admin logado.

use super::*;
use cardeal_kernel::{Preco, Quantidade};
use mod_clientes::{CriarPessoa as CriarCliente, PessoaCadastrada as ClienteCriado};
use mod_estoque::{
    CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, GrupoProdutoCriado, LocalCriado,
    ProdutoCriado, RegistrarEntrada, TipoLocal, UnidadeCriada,
};

/// Abre um motor real (SQLite em arquivo temporário) com o admin logado, cria cliente,
/// produto, local e 5 unidades em estoque, e abre uma OS. Devolve o que os testes usam.
struct Cenario {
    _arquivo: tempfile::NamedTempFile,
    motor: MotorLocal,
    sessao: SessaoLocal,
    ctx: egui::Context,
    estado: EstadoTelaOs,
    os: Id,
    produto: Id,
}

fn cenario() -> Cenario {
    let crate::testes_comum::MotorDeTeste {
        _arquivo: arquivo,
        motor,
        sessao,
    } = crate::testes_comum::motor_de_teste();

    let cliente: ClienteCriado = motor
        .executar(
            &sessao,
            "clientes.criar_pessoa.v1",
            &CriarCliente {
                tipo: TipoPessoa::Fisica,
                nome: "Cliente Teste".to_owned(),
                nome_fantasia: None,
                papel_inicial: PapelCliente::Cliente,
                documento_tipo: Some(TipoDocumento::Cpf),
                documento_numero: Some("52998224725".to_owned()),
                data_nascimento: None,
                endereco: None,
                contato: None,
            },
        )
        .expect("cliente");
    let grupo: GrupoProdutoCriado = motor
        .executar(
            &sessao,
            "estoque.criar_grupo_produto.v1",
            &CriarGrupoProduto {
                codigo: "PECAS".to_owned(),
                nome: "Peças".to_owned(),
                pai: None,
            },
        )
        .expect("grupo");
    let unidade: UnidadeCriada = motor
        .executar(
            &sessao,
            "estoque.criar_unidade.v1",
            &CriarUnidade {
                sigla: "UN".to_owned(),
                nome: "Unidade".to_owned(),
                fracionavel: false,
            },
        )
        .expect("unidade");
    let produto: ProdutoCriado = motor
        .executar(
            &sessao,
            "estoque.criar_produto.v1",
            &CriarProduto {
                grupo_produto: grupo.grupo_produto,
                nome: "Tela original".to_owned(),
                ncm: "85076000".to_owned(),
                unidade_padrao: unidade.unidade,
                codigo_barras: None,
                detalhes_tecnicos: None,
            },
        )
        .expect("produto");
    let local: LocalCriado = motor
        .executar(
            &sessao,
            "estoque.criar_local.v1",
            &CriarLocal {
                nome: "Depósito".to_owned(),
                tipo: TipoLocal::Deposito,
            },
        )
        .expect("local");
    motor
        .executar(
            &sessao,
            "estoque.registrar_entrada.v1",
            &RegistrarEntrada {
                produto: produto.produto,
                local: local.local,
                quantidade: Quantidade::unidades(5),
                custo_unitario: Preco::reais(90),
            },
        )
        .expect("entrada");
    let aberta: OrdemServicoAberta = motor
        .executar(
            &sessao,
            "os.abrir_ordem_servico.v1",
            &AbrirOrdemServico {
                cliente: cliente.pessoa,
                equipamento: "Notebook".to_owned(),
                defeito_relatado: "Tela quebrada".to_owned(),
                tecnico_responsavel: sessao.usuario(),
                garantia_dias: 90,
            },
        )
        .expect("abrir OS");

    let mut estado = EstadoTelaOs::default();
    estado.carregar(&motor, &sessao);
    Cenario {
        _arquivo: arquivo,
        motor,
        sessao,
        ctx: egui::Context::default(),
        estado,
        os: aberta.ordem_servico,
        produto: produto.produto,
    }
}

fn disponivel(c: &Cenario) -> Quantidade {
    c.estado
        .produtos
        .iter()
        .find(|p| p.produto == c.produto)
        .expect("produto na lista")
        .disponivel
}

/// Laudo editável também em diagnóstico, e o formulário reabre preenchido com o atual.
#[test]
fn laudo_e_corrigido_em_diagnostico_e_o_formulario_reabre_preenchido() {
    let mut c = cenario();
    for (problema, diagnostico) in [
        ("Tela quebrada", None),
        ("Tela e flat quebrados", Some("Trocar")),
    ] {
        aplicar_e_recarregar(
            &c.ctx,
            &c.motor,
            &c.sessao,
            &mut c.estado,
            c.os,
            "os.registrar_laudo.v1",
            &RegistrarLaudo {
                ordem_servico: c.os,
                descricao_problema: problema.to_owned(),
                diagnostico: diagnostico.map(str::to_owned),
                tecnico: c.sessao.usuario(),
            },
            "ok",
        );
    }
    let d = c.estado.detalhe.as_ref().expect("detalhe");
    assert_eq!(d.ordem.estado, EstadoOs::EmDiagnostico);
    assert_eq!(c.estado.laudo_problema, "Tela e flat quebrados");
    assert_eq!(c.estado.laudo_diagnostico, "Trocar");
}

/// O caminho que antes travava: peça orçada, OS em execução, e a tela sem como aplicar.
/// Agora aplicar tira do estoque e libera "Concluir execução".
#[test]
fn aplicar_peca_pela_tela_baixa_o_estoque_e_destrava_a_conclusao() {
    let mut c = cenario();
    assert_eq!(
        c.estado.aplicar_local,
        c.estado.locais.first().map(|l| l.id)
    );

    let item_peca: Id = c
        .motor
        .executar(
            &c.sessao,
            "os.montar_orcamento.v1",
            &MontarOrcamentoOs {
                ordem_servico: c.os,
                item: ItemOrcamentoNovo::Peca {
                    produto: c.produto,
                    quantidade: Quantidade::unidades(2),
                    preco_unitario: Preco::reais(160),
                },
            },
        )
        .expect("peça no orçamento");
    let ordem_servico = c.os;
    c.motor
        .executar(
            &c.sessao,
            "os.enviar_para_aprovacao.v1",
            &EnviarParaAprovacao { ordem_servico },
        )
        .expect("enviar para aprovação");
    c.motor
        .executar(
            &c.sessao,
            "os.aprovar_orcamento.v1",
            &AprovarOrcamentoOs {
                ordem_servico,
                identificacao_aprovador: "Cliente Teste".to_owned(),
            },
        )
        .expect("aprovar");
    c.motor
        .executar(
            &c.sessao,
            "os.iniciar_execucao.v1",
            &IniciarExecucao { ordem_servico },
        )
        .expect("iniciar execução");
    c.estado.abrir_detalhe(&c.motor, &c.sessao, c.os);
    assert_eq!(disponivel(&c), Quantidade::unidades(5));

    aplicar_pecas(
        &c.ctx,
        &c.motor,
        &c.sessao,
        &mut c.estado,
        c.os,
        &[item_peca],
    );

    let d = c.estado.detalhe.as_ref().expect("detalhe");
    assert!(
        d.itens_peca.iter().all(|i| i.aplicada),
        "a peça deveria constar aplicada"
    );
    assert_eq!(
        disponivel(&c),
        Quantidade::unidades(3),
        "duas unidades saíram do estoque"
    );
    c.motor
        .executar(
            &c.sessao,
            "os.concluir_execucao.v1",
            &ConcluirExecucao {
                ordem_servico: c.os,
            },
        )
        .expect("com a peça aplicada, a execução conclui");
}

/// Remover um item errado do orçamento devolve o total.
#[test]
fn remover_item_pela_tela_desconta_do_total() {
    let mut c = cenario();
    let item: Id = c
        .motor
        .executar(
            &c.sessao,
            "os.montar_orcamento.v1",
            &MontarOrcamentoOs {
                ordem_servico: c.os,
                item: ItemOrcamentoNovo::MaoDeObra {
                    descricao: "Serviço lançado errado".to_owned(),
                    valor: "80,00".parse().expect("valor"),
                    tecnico: c.sessao.usuario(),
                    horas: None,
                },
            },
        )
        .expect("mão de obra");
    c.estado.abrir_detalhe(&c.motor, &c.sessao, c.os);
    assert!(!c
        .estado
        .detalhe
        .as_ref()
        .expect("detalhe")
        .ordem
        .valor_total
        .e_zero());

    aplicar_e_recarregar(
        &c.ctx,
        &c.motor,
        &c.sessao,
        &mut c.estado,
        c.os,
        "os.remover_item_orcamento.v1",
        &RemoverItemOrcamento {
            ordem_servico: c.os,
            item,
            tipo: TipoItemOrcamento::MaoDeObra,
        },
        "removido",
    );
    let d = c.estado.detalhe.as_ref().expect("detalhe");
    assert!(d.itens_mao_de_obra.is_empty());
    assert!(d.ordem.valor_total.e_zero());
}

/// Lança R$ 300 de mão de obra na OS do cenário e abre o detalhe — ponto de partida dos
/// testes de faturamento.
fn com_mao_de_obra_de_300(c: &mut Cenario) {
    let _: Id = c
        .motor
        .executar(
            &c.sessao,
            "os.montar_orcamento.v1",
            &MontarOrcamentoOs {
                ordem_servico: c.os,
                item: ItemOrcamentoNovo::MaoDeObra {
                    descricao: "Troca de tela".to_owned(),
                    valor: "300,00".parse().expect("valor"),
                    tecnico: c.sessao.usuario(),
                    horas: None,
                },
            },
        )
        .expect("mão de obra");
    c.estado.abrir_detalhe(&c.motor, &c.sessao, c.os);
}

#[test]
fn faturar_a_prazo_gera_as_parcelas_em_aberto_no_financeiro() {
    let mut c = cenario();
    com_mao_de_obra_de_300(&mut c);
    let os = c.estado.detalhe.as_ref().expect("detalhe").ordem.clone();

    c.estado.dlg = Dlg::faturar();
    if let Dlg::Faturar(p) = &mut c.estado.dlg {
        p.condicao.a_prazo = true;
        p.condicao.parcelas = "3".to_owned();
        p.condicao.primeiro_vencimento = Data::hoje(Fuso::BRASILIA).mais_dias(30).to_string();
        p.condicao.intervalo_dias = "30".to_owned();
    }
    faturar_os(&c.ctx, &c.motor, &c.sessao, &mut c.estado, &os);

    let d = c.estado.detalhe.as_ref().expect("detalhe");
    assert_eq!(d.ordem.estado, EstadoOs::Faturada);
    let abertas: Vec<mod_financeiro::ItemTituloEmAberto> = c
        .motor
        .consultar(
            &c.sessao,
            "financeiro.titulos_a_receber_em_aberto.v1",
            &mod_financeiro::TitulosAReceberEmAberto,
        )
        .expect("em aberto");
    assert_eq!(abertas.len(), 3);
    let total = abertas
        .iter()
        .fold(Dinheiro::ZERO, |acc, p| acc + p.saldo());
    assert_eq!(total, "300,00".parse::<Dinheiro>().expect("valor"));
}

#[test]
fn faturar_a_vista_no_pix_cai_na_conta_bancaria_escolhida() {
    let mut c = cenario();
    com_mao_de_obra_de_300(&mut c);
    let os = c.estado.detalhe.as_ref().expect("detalhe").ordem.clone();
    let criar = |nome: &str| -> Id {
        let r: mod_financeiro::ContaBancariaCriada = c
            .motor
            .executar(
                &c.sessao,
                "financeiro.criar_conta_bancaria.v1",
                &mod_financeiro::CriarContaBancaria {
                    nome: nome.to_owned(),
                },
            )
            .expect("conta");
        r.conta
    };
    let _nubank = criar("Nubank");
    let inter = criar("Inter");

    c.estado.dlg = Dlg::faturar();
    if let Dlg::Faturar(p) = &mut c.estado.dlg {
        p.condicao.meio = Some(MeioPagamento::Pix);
        p.condicao.conta = Some(inter);
    }
    faturar_os(&c.ctx, &c.motor, &c.sessao, &mut c.estado, &os);

    let contas: Vec<mod_financeiro::ItemContaDisponivel> = c
        .motor
        .consultar(
            &c.sessao,
            "financeiro.contas_disponiveis.v1",
            &mod_financeiro::ContasDisponiveis,
        )
        .expect("contas");
    let saldo = |id: Id| contas.iter().find(|x| x.conta == id).expect("conta").saldo;
    assert_eq!(saldo(inter), "300,00".parse::<Dinheiro>().expect("valor"));
    assert!(saldo(_nubank).e_zero());
}

#[test]
fn nova_os_com_cliente_novo_pela_tela_cadastra_e_abre_juntos() {
    let mut c = cenario();
    let clientes_antes = c.estado.clientes.len();
    c.estado.dlg = Dlg::nova();
    if let Dlg::Nova {
        cliente_novo,
        nome,
        telefone,
        email,
        equipamento,
        defeito_relatado,
        ..
    } = &mut c.estado.dlg
    {
        *cliente_novo = true;
        *nome = "Oficina do Zé".to_owned();
        *telefone = "31999990000".to_owned();
        *email = "ze@oficina.com".to_owned();
        *equipamento = "Parafusadeira Makita".to_owned();
        *defeito_relatado = "Bateria não segura carga".to_owned();
    }
    abrir_os(&c.ctx, &c.motor, &c.sessao, &mut c.estado);

    assert_eq!(c.estado.clientes.len(), clientes_antes + 1);
    let d = c.estado.detalhe.as_ref().expect("detalhe da OS aberta");
    assert_eq!(d.ordem.equipamento, "Parafusadeira Makita");
    let novo = c
        .estado
        .clientes
        .iter()
        .find(|p| p.nome == "Oficina do Zé")
        .expect("cliente novo");
    assert_eq!(d.ordem.cliente, novo.pessoa);
}

#[test]
fn busca_da_lista_acha_pelo_nome_do_cliente_e_respeita_o_status() {
    let mut c = cenario();
    c.estado.busca = "cliente teste".to_owned();
    c.estado.buscar_ordens(&c.motor, &c.sessao);
    assert_eq!(c.estado.ordens.len(), 1);

    c.estado.busca = "ninguém com esse nome".to_owned();
    c.estado.buscar_ordens(&c.motor, &c.sessao);
    assert!(c.estado.ordens.is_empty());

    c.estado.busca.clear();
    c.estado.filtro_status = Some(FiltroStatusOs::Um(EstadoOs::Faturada));
    c.estado.buscar_ordens(&c.motor, &c.sessao);
    assert!(c.estado.ordens.is_empty());
    // Os indicadores continuam olhando a fila ativa inteira.
    assert_eq!(c.estado.ativas.len(), 1);
}
