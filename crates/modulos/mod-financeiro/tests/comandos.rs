//! Os comandos do financeiro atravessando o `Despachante` real contra SQLite: autorização,
//! transação, receituário no Razão e persistência — tudo junto.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel, Sessao};
use cardeal_kernel::{CodigoErro, Data, Dinheiro, Fuso, Id, Instante, Periodo};
use cardeal_ledger::{semear_plano_padrao, Contas, Contraparte, PapelConta, RepositorioRazao};
use cardeal_modkit::{Ambiente, Ctx, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_financeiro::{
    materializar_recorrencias_pendentes, AbrirCaixa, BaixarPagamento, BaixarRecebimento,
    CadastrarCaixa, CaixaFoiAberto, CaixaFoiFechado, CategoriaCriada, CategoriaFinanceira,
    Categorias, ConstrutorTitulo, CriarCategoria, CriarContaBancaria, CriarRecorrencia,
    EspecieTitulo, EstornarBaixa, FecharCaixa, ItemTituloEmAberto, ItemTotalPorCategoria,
    LancarTituloAPagar, LancarTituloAReceber, ModuloFinanceiro, PagamentoBaixado, Periodicidade,
    PoliticaJuros, RecebimentoBaixado, RecorrenciaCriada, RegistrarSangria, RegistrarSuprimento,
    RenegociarTitulo, RepositorioFinanceiro, SangriaFoiRegistrada, SuprimentoFoiRegistrado,
    TipoValor, TituloAPagarLancado, TituloAReceberLancado, TituloDaOrigem, TitulosAReceberEmAberto,
    TotalPorCategoriaNoPeriodo, MANIFESTO,
};
use serde::Serialize;
use tempfile::TempDir;

fn liga(e: cardeal_ledger::ErroRazao) -> ErroArmazenamento {
    ErroArmazenamento::Sqlite(e.to_string())
}

fn base() -> (TempDir, Armazenamento, Id) {
    let dir = tempfile::tempdir().unwrap();
    let arm =
        Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("cardeal.db"))).unwrap();
    arm.migrar(&[
        cardeal_ledger::migracoes::conjunto(),
        ModuloFinanceiro.migracoes(),
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
                     VALUES (?1,'Teste LTDA','Teste','11222333000181','SimplesNacional','{}','comercio',0)",
                    [empresa.em_bytes().as_slice()],
                )
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
            semear_plano_padrao(uow, empresa).map_err(liga)
        })
        .unwrap();

    (dir, arm, empresa)
}

fn ambiente(empresa: Id) -> Ambiente {
    let mut rm = RegistroModulos::novo();
    rm.registrar(&MANIFESTO).unwrap();
    let efetivo = rm
        .resolver(&PedidoAtivacao::nova().com_modulo("financeiro"))
        .unwrap();
    Ambiente::novo(empresa, efetivo)
}

fn sessao(empresa: Id, permissoes: &[&str]) -> Sessao {
    let mut papel = Papel::novo(empresa, "Financeiro");
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

/// Um `Ctx` de verdade, montado sem passar pelo lookup por nome do `Despachante` — o que
/// `materializar_recorrencias_pendentes` exige (não é `Comando`: `docs/modulos/
/// financeiro.md` §5 já a descrevia como "tarefa agendada, sem permissão de usuário").
fn ctx_direto(empresa: Id) -> Ctx {
    let mut rm = RegistroModulos::novo();
    rm.registrar(&MANIFESTO).unwrap();
    let efetivo = rm
        .resolver(&PedidoAtivacao::nova().com_modulo("financeiro"))
        .unwrap();
    let ambiente = Ambiente::novo(empresa, efetivo);
    let sessao = Sessao::abrir(EmissaoSessao::padrao(
        Id::novo(),
        Id::novo(),
        Escopo::empresa_inteira(empresa),
        AutorizacoesEfetivas::default(),
        Instante::EPOCA,
    ));
    Ctx::de_sessao(&sessao, &ambiente)
}

fn hoje() -> Data {
    Data::hoje(Fuso::BRASILIA)
}

fn carga(v: &impl Serialize) -> Vec<u8> {
    postcard::to_stdvec(v).unwrap()
}

fn conta_lancamentos(arm: &Armazenamento, empresa: Id) -> i64 {
    arm.leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM razao_lancamento WHERE empresa = ?1",
                [empresa.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap()
}

fn conta_baixas(arm: &Armazenamento, empresa: Id) -> i64 {
    arm.leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM financeiro_baixa WHERE empresa = ?1",
                [empresa.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap()
}

/// A conta analítica de papel `Caixa` que `semear_plano_padrao` já deixou pronta — é a que os
/// testes de caixa usam como `conta_razao` de um `Caixa` recém-cadastrado.
fn conta_papel_caixa(arm: &Armazenamento, empresa: Id) -> Id {
    let bytes: Vec<u8> = arm
        .leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT id FROM razao_conta WHERE empresa = ?1 AND papel_padrao = 'Caixa'",
                [empresa.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap();
    Id::de_bytes(bytes.try_into().expect("id tem 16 bytes"))
}

fn lancar(
    d: &Despachante,
    arm: &Armazenamento,
    empresa: Id,
    s: &Sessao,
    valor: Dinheiro,
    parcelas: u16,
) -> TituloAReceberLancado {
    let cmd = LancarTituloAReceber {
        cliente: Id::novo(),
        valor_total: valor,
        emissao: hoje(),
        parcelas,
        primeiro_vencimento: hoje(),
        intervalo_dias: 30,
        observacao: None,
        categoria: None,
    };
    let saida = d
        .executar_comando(
            "financeiro.lancar_titulo_a_receber.v1",
            &carga(&cmd),
            s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    postcard::from_bytes(&saida).unwrap()
}

#[test]
fn lancar_titulo_cria_parcelas_e_um_lancamento_confirmado_por_parcela() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(empresa, &["financeiro.receber.criar"]);

    let out = lancar(&d, &arm, empresa, &s, Dinheiro::reais(90), 3);
    assert_eq!(out.parcelas.len(), 3);
    assert_eq!(out.lancamentos.len(), 3);
    assert_eq!(conta_lancamentos(&arm, empresa), 3);
}

#[test]
fn baixa_em_dia_quita_a_parcela_e_gera_lancamento_realizado() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &["financeiro.receber.criar", "financeiro.receber.baixar"],
    );

    let titulo = lancar(&d, &arm, empresa, &s, Dinheiro::reais(100), 1);
    let cmd = BaixarRecebimento {
        parcela: titulo.parcelas[0],
        valor: Dinheiro::reais(100),
        data: hoje(),
        conta_destino: None,
    };
    let saida = d
        .executar_comando(
            "financeiro.baixar_recebimento.v1",
            &carga(&cmd),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let baixada: RecebimentoBaixado = postcard::from_bytes(&saida).unwrap();

    assert!(baixada.parcela_quitada);
    assert_eq!(baixada.saldo_restante, Dinheiro::ZERO);
    assert_eq!(conta_baixas(&arm, empresa), 1);
    // 1 lançamento Confirmado (título) + 1 Realizado (baixa).
    assert_eq!(conta_lancamentos(&arm, empresa), 2);
}

#[test]
fn baixa_parcial_deixa_saldo_e_nao_quita() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &["financeiro.receber.criar", "financeiro.receber.baixar"],
    );

    let titulo = lancar(&d, &arm, empresa, &s, Dinheiro::reais(100), 1);
    let cmd = BaixarRecebimento {
        parcela: titulo.parcelas[0],
        valor: Dinheiro::reais(40),
        data: hoje(),
        conta_destino: None,
    };
    let saida = d
        .executar_comando(
            "financeiro.baixar_recebimento.v1",
            &carga(&cmd),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let baixada: RecebimentoBaixado = postcard::from_bytes(&saida).unwrap();
    assert!(!baixada.parcela_quitada);
    assert_eq!(baixada.saldo_restante, Dinheiro::reais(60));
}

#[test]
fn sem_a_permissao_de_criar_o_lancamento_e_recusado() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(empresa, &["financeiro.receber.ver"]); // não pode criar

    let cmd = LancarTituloAReceber {
        cliente: Id::novo(),
        valor_total: Dinheiro::reais(10),
        emissao: hoje(),
        parcelas: 1,
        primeiro_vencimento: hoje(),
        intervalo_dias: 30,
        observacao: None,
        categoria: None,
    };
    let erro = d
        .executar_comando(
            "financeiro.lancar_titulo_a_receber.v1",
            &carga(&cmd),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::SEM_PERMISSAO);
    assert_eq!(conta_lancamentos(&arm, empresa), 0);
}

#[test]
fn baixa_acima_do_devido_falha_e_desfaz_tudo() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &["financeiro.receber.criar", "financeiro.receber.baixar"],
    );

    let titulo = lancar(&d, &arm, empresa, &s, Dinheiro::reais(100), 1);
    let antes = conta_lancamentos(&arm, empresa);

    let cmd = BaixarRecebimento {
        parcela: titulo.parcelas[0],
        valor: Dinheiro::reais(150),
        data: hoje(),
        conta_destino: None,
    };
    let erro = d
        .executar_comando(
            "financeiro.baixar_recebimento.v1",
            &carga(&cmd),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::REGRA_VIOLADA);
    // Nada foi gravado: nem o lançamento Realizado, nem a baixa.
    assert_eq!(conta_lancamentos(&arm, empresa), antes);
    assert_eq!(conta_baixas(&arm, empresa), 0);
}

#[test]
fn fluxo_a_pagar_espelha_o_a_receber() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &["financeiro.pagar.criar", "financeiro.pagar.baixar"],
    );

    let cmd = LancarTituloAPagar {
        fornecedor: Id::novo(),
        valor_total: Dinheiro::reais(500),
        emissao: hoje(),
        parcelas: 2,
        primeiro_vencimento: hoje(),
        intervalo_dias: 30,
        observacao: Some("conta de luz".into()),
        categoria: None,
    };
    let saida = d
        .executar_comando(
            "financeiro.lancar_titulo_a_pagar.v1",
            &carga(&cmd),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let titulo: TituloAPagarLancado = postcard::from_bytes(&saida).unwrap();
    assert_eq!(titulo.parcelas.len(), 2);
    assert_eq!(conta_lancamentos(&arm, empresa), 2);

    let baixa = BaixarPagamento {
        parcela: titulo.parcelas[0],
        valor: Dinheiro::reais(250),
        data: hoje(),
        conta_destino: None,
    };
    let saida = d
        .executar_comando(
            "financeiro.baixar_pagamento.v1",
            &carga(&baixa),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let baixado: PagamentoBaixado = postcard::from_bytes(&saida).unwrap();
    assert!(baixado.parcela_quitada);
    assert_eq!(baixado.saldo_restante, Dinheiro::ZERO);
    assert_eq!(conta_baixas(&arm, empresa), 1);
    assert_eq!(conta_lancamentos(&arm, empresa), 3);
}

fn cadastrar_caixa(d: &Despachante, arm: &Armazenamento, empresa: Id, s: &Sessao) -> Id {
    let cmd = CadastrarCaixa {
        nome: "Caixa 1".to_string(),
        local_operacao: None,
        conta_razao: conta_papel_caixa(arm, empresa),
        permite_negativo: false,
    };
    let saida = d
        .executar_comando(
            "financeiro.cadastrar_caixa.v1",
            &carga(&cmd),
            s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    postcard::from_bytes::<mod_financeiro::CaixaCadastrado>(&saida)
        .unwrap()
        .caixa
}

#[test]
fn ciclo_completo_de_caixa_abre_suprimenta_sangra_e_fecha_sem_quebra() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &[
            "financeiro.caixa.cadastrar",
            "financeiro.caixa.abrir",
            "financeiro.caixa.suprimento",
            "financeiro.caixa.sangria",
            "financeiro.caixa.fechar",
        ],
    );

    let caixa = cadastrar_caixa(&d, &arm, empresa, &s);

    let abertura = AbrirCaixa {
        caixa,
        valor_abertura: Dinheiro::reais(100),
    };
    let saida = d
        .executar_comando(
            "financeiro.abrir_caixa.v1",
            &carga(&abertura),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let aberto: CaixaFoiAberto = postcard::from_bytes(&saida).unwrap();
    assert!(aberto.lancamento_suprimento.is_some());

    let suprimento = RegistrarSuprimento {
        sessao: aberto.sessao,
        valor: Dinheiro::reais(50),
    };
    let saida = d
        .executar_comando(
            "financeiro.registrar_suprimento.v1",
            &carga(&suprimento),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let _: SuprimentoFoiRegistrado = postcard::from_bytes(&saida).unwrap();

    let sangria = RegistrarSangria {
        sessao: aberto.sessao,
        valor: Dinheiro::reais(30),
        motivo: "Troco para o cofre".to_string(),
    };
    let saida = d
        .executar_comando(
            "financeiro.registrar_sangria.v1",
            &carga(&sangria),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let _: SangriaFoiRegistrada = postcard::from_bytes(&saida).unwrap();

    // 100 (abertura) + 50 (suprimento) − 30 (sangria) = 120, sem quebra.
    let fechamento = FecharCaixa {
        sessao: aberto.sessao,
        valor_contado: Dinheiro::reais(120),
        motivo: None,
    };
    let saida = d
        .executar_comando(
            "financeiro.fechar_caixa.v1",
            &carga(&fechamento),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let fechado: CaixaFoiFechado = postcard::from_bytes(&saida).unwrap();
    assert_eq!(fechado.valor_esperado, Dinheiro::reais(120));
    assert_eq!(fechado.quebra, Dinheiro::ZERO);
    assert!(fechado.lancamento_ajuste.is_none());
}

#[test]
fn abrir_caixa_recusa_uma_segunda_sessao_no_mesmo_caixa() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &["financeiro.caixa.cadastrar", "financeiro.caixa.abrir"],
    );
    let caixa = cadastrar_caixa(&d, &arm, empresa, &s);

    let abrir = AbrirCaixa {
        caixa,
        valor_abertura: Dinheiro::ZERO,
    };
    d.executar_comando(
        "financeiro.abrir_caixa.v1",
        &carga(&abrir),
        &s,
        &ambiente(empresa),
        arm.escritor(),
    )
    .unwrap();

    let erro = d
        .executar_comando(
            "financeiro.abrir_caixa.v1",
            &carga(&abrir),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ESTADO_INVALIDO);
}

#[test]
fn sangria_acima_do_saldo_do_caixa_e_recusada() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &[
            "financeiro.caixa.cadastrar",
            "financeiro.caixa.abrir",
            "financeiro.caixa.sangria",
        ],
    );
    let caixa = cadastrar_caixa(&d, &arm, empresa, &s);
    let abertura = AbrirCaixa {
        caixa,
        valor_abertura: Dinheiro::reais(100),
    };
    let saida = d
        .executar_comando(
            "financeiro.abrir_caixa.v1",
            &carga(&abertura),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let aberto: CaixaFoiAberto = postcard::from_bytes(&saida).unwrap();

    let sangria = RegistrarSangria {
        sessao: aberto.sessao,
        valor: Dinheiro::reais(150),
        motivo: "tentativa acima do saldo".to_string(),
    };
    let erro = d
        .executar_comando(
            "financeiro.registrar_sangria.v1",
            &carga(&sangria),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::REGRA_VIOLADA);
}

#[test]
fn fechamento_com_quebra_acima_da_tolerancia_exige_motivo_e_gera_ajuste() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &[
            "financeiro.caixa.cadastrar",
            "financeiro.caixa.abrir",
            "financeiro.caixa.fechar",
        ],
    );
    let caixa = cadastrar_caixa(&d, &arm, empresa, &s);
    let abertura = AbrirCaixa {
        caixa,
        valor_abertura: Dinheiro::reais(100),
    };
    let saida = d
        .executar_comando(
            "financeiro.abrir_caixa.v1",
            &carga(&abertura),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let aberto: CaixaFoiAberto = postcard::from_bytes(&saida).unwrap();

    let sem_motivo = FecharCaixa {
        sessao: aberto.sessao,
        valor_contado: Dinheiro::reais(80),
        motivo: None,
    };
    let erro = d
        .executar_comando(
            "financeiro.fechar_caixa.v1",
            &carga(&sem_motivo),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::REGRA_VIOLADA);

    let com_motivo = FecharCaixa {
        sessao: aberto.sessao,
        valor_contado: Dinheiro::reais(80),
        motivo: Some("gaveta batida errada no troco".to_string()),
    };
    let saida = d
        .executar_comando(
            "financeiro.fechar_caixa.v1",
            &carga(&com_motivo),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let fechado: CaixaFoiFechado = postcard::from_bytes(&saida).unwrap();
    assert_eq!(fechado.quebra, Dinheiro::reais(-20));
    assert!(fechado.lancamento_ajuste.is_some());
}

#[test]
fn baixar_pagamento_recusa_uma_parcela_a_receber() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &["financeiro.receber.criar", "financeiro.pagar.baixar"],
    );

    let titulo = lancar(&d, &arm, empresa, &s, Dinheiro::reais(100), 1);
    let cmd = BaixarPagamento {
        parcela: titulo.parcelas[0],
        valor: Dinheiro::reais(100),
        data: hoje(),
        conta_destino: None,
    };
    let erro = d
        .executar_comando(
            "financeiro.baixar_pagamento.v1",
            &carga(&cmd),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::REGRA_VIOLADA);
    assert_eq!(conta_baixas(&arm, empresa), 0);
}

#[test]
fn titulos_a_receber_em_aberto_lista_por_vencimento_e_ignora_quitados() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &[
            "financeiro.receber.criar",
            "financeiro.receber.baixar",
            "financeiro.receber.ver",
        ],
    );

    let aberto = lancar(&d, &arm, empresa, &s, Dinheiro::reais(100), 1);
    let quitado = lancar(&d, &arm, empresa, &s, Dinheiro::reais(50), 1);
    let cmd = BaixarRecebimento {
        parcela: quitado.parcelas[0],
        valor: Dinheiro::reais(50),
        data: hoje(),
        conta_destino: None,
    };
    d.executar_comando(
        "financeiro.baixar_recebimento.v1",
        &carga(&cmd),
        &s,
        &ambiente(empresa),
        arm.escritor(),
    )
    .unwrap();

    let saida = d
        .executar_consulta(
            "financeiro.titulos_a_receber_em_aberto.v1",
            &carga(&TitulosAReceberEmAberto),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap();
    let itens: Vec<ItemTituloEmAberto> = postcard::from_bytes(&saida).unwrap();

    assert_eq!(itens.len(), 1);
    assert_eq!(itens[0].titulo, aberto.titulo);
    assert_eq!(itens[0].saldo(), Dinheiro::reais(100));
}

#[test]
fn titulos_a_receber_em_aberto_sem_permissao_e_recusado() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(empresa, &[]);

    let erro = d
        .executar_consulta(
            "financeiro.titulos_a_receber_em_aberto.v1",
            &carga(&TitulosAReceberEmAberto),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::SEM_PERMISSAO);
}

#[test]
fn estornar_baixa_reverte_o_lancamento_e_reabre_a_parcela() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &[
            "financeiro.receber.criar",
            "financeiro.receber.baixar",
            "financeiro.receber.estornar",
        ],
    );

    let titulo = lancar(&d, &arm, empresa, &s, Dinheiro::reais(100), 1);
    let baixada: RecebimentoBaixado = postcard::from_bytes(
        &d.executar_comando(
            "financeiro.baixar_recebimento.v1",
            &carga(&BaixarRecebimento {
                parcela: titulo.parcelas[0],
                valor: Dinheiro::reais(100),
                data: hoje(),
                conta_destino: None,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(baixada.parcela_quitada);
    // 1 Confirmado (título) + 1 Realizado (baixa).
    assert_eq!(conta_lancamentos(&arm, empresa), 2);

    let estornada: mod_financeiro::BaixaFoiEstornada = postcard::from_bytes(
        &d.executar_comando(
            "financeiro.estornar_baixa.v1",
            &carga(&EstornarBaixa {
                baixa: baixada.baixa,
                motivo: "cliente pagou por engano, devolvido em dinheiro".to_string(),
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(estornada.parcela, titulo.parcelas[0]);
    // + 1 lançamento de estorno.
    assert_eq!(conta_lancamentos(&arm, empresa), 3);

    // A parcela reabriu — uma nova baixa total volta a funcionar.
    let rebaixada: RecebimentoBaixado = postcard::from_bytes(
        &d.executar_comando(
            "financeiro.baixar_recebimento.v1",
            &carga(&BaixarRecebimento {
                parcela: titulo.parcelas[0],
                valor: Dinheiro::reais(100),
                data: hoje(),
                conta_destino: None,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(rebaixada.parcela_quitada);

    // Estornar a mesma baixa de novo é recusado (idempotência).
    let erro = d
        .executar_comando(
            "financeiro.estornar_baixa.v1",
            &carga(&EstornarBaixa {
                baixa: baixada.baixa,
                motivo: "tentativa de estorno duplicado".to_string(),
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ESTADO_INVALIDO);
}

#[test]
fn renegociar_titulo_consolida_o_saldo_em_aberto_num_titulo_novo() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &[
            "financeiro.receber.criar",
            "financeiro.receber.baixar",
            "financeiro.receber.renegociar",
        ],
    );

    let titulo = lancar(&d, &arm, empresa, &s, Dinheiro::reais(200), 2);
    // Quita a primeira parcela; a segunda (100) fica em aberto.
    d.executar_comando(
        "financeiro.baixar_recebimento.v1",
        &carga(&BaixarRecebimento {
            parcela: titulo.parcelas[0],
            valor: Dinheiro::reais(100),
            data: hoje(),
            conta_destino: None,
        }),
        &s,
        &ambiente(empresa),
        arm.escritor(),
    )
    .unwrap();
    let lancamentos_antes = conta_lancamentos(&arm, empresa);

    let renegociado: mod_financeiro::TituloFoiRenegociado = postcard::from_bytes(
        &d.executar_comando(
            "financeiro.renegociar_titulo.v1",
            &carga(&RenegociarTitulo {
                titulo: titulo.titulo,
                numero_parcelas: 1,
                primeiro_vencimento: hoje().mais_dias(60),
                intervalo_dias: 0,
                politica_juros: PoliticaJuros::Nenhum,
                taxa_juros: None,
                multa: None,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(renegociado.saldo, Dinheiro::reais(100));
    assert_ne!(renegociado.titulo_novo, titulo.titulo);
    // Renegociação não posta no Razão — a receita já foi reconhecida na emissão original.
    assert_eq!(conta_lancamentos(&arm, empresa), lancamentos_antes);

    // Sem saldo em aberto (título já totalmente absorvido/quitado), renegociar é recusado.
    let erro = d
        .executar_comando(
            "financeiro.renegociar_titulo.v1",
            &carga(&RenegociarTitulo {
                titulo: titulo.titulo,
                numero_parcelas: 1,
                primeiro_vencimento: hoje().mais_dias(60),
                intervalo_dias: 0,
                politica_juros: PoliticaJuros::Nenhum,
                taxa_juros: None,
                multa: None,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::REGRA_VIOLADA);
}

#[test]
fn criar_conta_bancaria_abre_contas_sucessivas_e_recebe_baixa() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &[
            "financeiro.banco.criar",
            "financeiro.receber.criar",
            "financeiro.receber.baixar",
        ],
    );

    let primeira: mod_financeiro::ContaBancariaCriada = postcard::from_bytes(
        &d.executar_comando(
            "financeiro.criar_conta_bancaria.v1",
            &carga(&CriarContaBancaria {
                nome: "Nubank".to_string(),
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(primeira.codigo, "1.1.05");

    let segunda: mod_financeiro::ContaBancariaCriada = postcard::from_bytes(
        &d.executar_comando(
            "financeiro.criar_conta_bancaria.v1",
            &carga(&CriarContaBancaria {
                nome: "Banco do Brasil".to_string(),
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(segunda.codigo, "1.1.06");
    assert_ne!(primeira.conta, segunda.conta);

    // A conta nova recebe baixa de verdade, como o Caixa.
    let titulo = lancar(&d, &arm, empresa, &s, Dinheiro::reais(300), 1);
    let baixada: RecebimentoBaixado = postcard::from_bytes(
        &d.executar_comando(
            "financeiro.baixar_recebimento.v1",
            &carga(&BaixarRecebimento {
                parcela: titulo.parcelas[0],
                valor: Dinheiro::reais(300),
                data: hoje(),
                conta_destino: Some(primeira.conta),
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(baixada.parcela_quitada);
}

fn conta_papel(arm: &Armazenamento, empresa: Id, papel: PapelConta) -> Id {
    let ctx = ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx, move |uow| {
            let repo = RepositorioRazao::novo(uow);
            Contas::nova(&repo, empresa).papel(papel).map_err(liga)
        })
        .unwrap()
        .valor()
}

#[test]
fn criar_categoria_aparece_na_lista_de_ativas() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &["financeiro.categoria.criar", "financeiro.categoria.ver"],
    );

    let criada: CategoriaCriada = postcard::from_bytes(
        &d.executar_comando(
            "financeiro.criar_categoria.v1",
            &carga(&CriarCategoria {
                nome: "Aluguel".to_string(),
                especie: None,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let lista: Vec<CategoriaFinanceira> = postcard::from_bytes(
        &d.executar_consulta(
            "financeiro.categorias.v1",
            &carga(&Categorias),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(lista.len(), 1);
    assert_eq!(lista[0].id, criada.categoria);
    assert_eq!(lista[0].nome, "Aluguel");

    // Nome vazio é recusado.
    let erro = d
        .executar_comando(
            "financeiro.criar_categoria.v1",
            &carga(&CriarCategoria {
                nome: "   ".to_string(),
                especie: None,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ENTRADA_INVALIDA);
}

#[test]
fn criar_recorrencia_nao_lanca_na_hora_e_materializar_e_idempotente() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &["financeiro.recorrencia.criar", "financeiro.categoria.criar"],
    );
    let conta_despesa = conta_papel(&arm, empresa, PapelConta::DespesaAdministrativa);

    let categoria: CategoriaCriada = postcard::from_bytes(
        &d.executar_comando(
            "financeiro.criar_categoria.v1",
            &carga(&CriarCategoria {
                nome: "Aluguel".to_string(),
                especie: None,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let fornecedor = Id::novo();
    let _criada: RecorrenciaCriada = postcard::from_bytes(
        &d.executar_comando(
            "financeiro.criar_recorrencia.v1",
            &carga(&CriarRecorrencia {
                descricao: "Aluguel da loja".to_string(),
                especie: mod_financeiro::EspecieTitulo::Pagar,
                contraparte: Contraparte::Fornecedor(fornecedor),
                tipo_valor: TipoValor::Fixo,
                valor_fixo: Some(Dinheiro::reais(2200)),
                indice: None,
                media_ultimos_n: None,
                periodicidade: Periodicidade::Mensal,
                dia_referencia: Some(1),
                expressao_cron: None,
                inicio: hoje(),
                fim: None,
                conta_contrapartida: conta_despesa,
                centro_custo: None,
                categoria: Some(categoria.categoria),
                antecedencia_geracao_dias: 35,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    // Nenhum título nasce ao criar a regra.
    assert_eq!(conta_lancamentos(&arm, empresa), 0);

    let ctx = ctx_direto(empresa);
    let gerados: Vec<Id> = arm
        .escritor()
        .executar(
            ContextoEscrita::novo(empresa, ctx.usuario, ctx.dispositivo, Id::novo()),
            move |uow| materializar_recorrencias_pendentes(&ctx, uow).map_err(liga_erro),
        )
        .unwrap()
        .valor();
    assert_eq!(gerados.len(), 1);
    assert_eq!(conta_lancamentos(&arm, empresa), 1);

    // Rodar de novo no mesmo dia não duplica (a checagem é contra o título já gravado).
    let ctx2 = ctx_direto(empresa);
    let gerados2: Vec<Id> = arm
        .escritor()
        .executar(
            ContextoEscrita::novo(empresa, ctx2.usuario, ctx2.dispositivo, Id::novo()),
            move |uow| materializar_recorrencias_pendentes(&ctx2, uow).map_err(liga_erro),
        )
        .unwrap()
        .valor();
    assert!(gerados2.is_empty());
    assert_eq!(conta_lancamentos(&arm, empresa), 1);
}

fn liga_erro(e: cardeal_kernel::Erro) -> ErroArmazenamento {
    ErroArmazenamento::Sqlite(e.to_string())
}

#[test]
fn total_por_categoria_no_periodo_agrega_o_que_foi_baixado() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &[
            "financeiro.receber.criar",
            "financeiro.receber.baixar",
            "financeiro.categoria.criar",
            "financeiro.categoria.ver",
        ],
    );

    let categoria: CategoriaCriada = postcard::from_bytes(
        &d.executar_comando(
            "financeiro.criar_categoria.v1",
            &carga(&CriarCategoria {
                nome: "Assinatura SaaS — Cliente X".to_string(),
                especie: Some(mod_financeiro::EspecieTitulo::Receber),
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let cmd = LancarTituloAReceber {
        cliente: Id::novo(),
        valor_total: Dinheiro::reais(500),
        emissao: hoje(),
        parcelas: 1,
        primeiro_vencimento: hoje(),
        intervalo_dias: 30,
        observacao: None,
        categoria: Some(categoria.categoria),
    };
    let lancado: TituloAReceberLancado = postcard::from_bytes(
        &d.executar_comando(
            "financeiro.lancar_titulo_a_receber.v1",
            &carga(&cmd),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    d.executar_comando(
        "financeiro.baixar_recebimento.v1",
        &carga(&BaixarRecebimento {
            parcela: lancado.parcelas[0],
            valor: Dinheiro::reais(500),
            data: hoje(),
            conta_destino: None,
        }),
        &s,
        &ambiente(empresa),
        arm.escritor(),
    )
    .unwrap();

    let totais: Vec<ItemTotalPorCategoria> = postcard::from_bytes(
        &d.executar_consulta(
            "financeiro.total_por_categoria_no_periodo.v1",
            &carga(&TotalPorCategoriaNoPeriodo {
                periodo: Periodo::novo(hoje().mais_dias(-1), hoje().mais_dias(1)),
            }),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(totais.len(), 1);
    assert_eq!(totais[0].categoria, Some(categoria.categoria));
    assert_eq!(totais[0].total_baixado, Dinheiro::reais(500));
    assert_eq!(totais[0].competencia, hoje().competencia());
}

#[test]
fn titulo_da_origem_encontra_o_titulo_vinculado_a_outro_modulo() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(empresa, &["financeiro.receber.ver"]);
    let amb = ambiente(empresa);
    let cliente = Id::novo();
    let ordem_de_servico = Id::novo();

    // Um título gravado do jeito que `mod-os::FaturarOrdemServico` grava — direto pelo
    // construtor + `inserir_titulo`, com `origem("os", Some(id))` — não pelo comando
    // `LancarTituloAReceber` (que sempre nasce "avulso").
    let ctx_escrita = ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo());
    let titulo_id = arm
        .escritor()
        .executar(ctx_escrita, move |uow| {
            let tcp = ConstrutorTitulo::novo(
                empresa,
                EspecieTitulo::Receber,
                Contraparte::Cliente(cliente),
                Dinheiro::reais(240),
                hoje(),
            )
            .origem("os", Some(ordem_de_servico))
            .parcelas(1, hoje(), 0)
            .construir()
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
            RepositorioFinanceiro::novo(uow).inserir_titulo(&tcp)?;
            Ok(tcp.titulo.id)
        })
        .unwrap()
        .valor;

    // Encontra pelo par origem_modulo/origem_id.
    let achado: Option<mod_financeiro::Titulo> = postcard::from_bytes(
        &d.executar_consulta(
            "financeiro.titulo_da_origem.v1",
            &carga(&TituloDaOrigem {
                origem_modulo: "os".to_string(),
                origem_id: ordem_de_servico,
            }),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(achado.unwrap().id, titulo_id);

    // Um id de origem diferente não encontra nada.
    let nada: Option<mod_financeiro::Titulo> = postcard::from_bytes(
        &d.executar_consulta(
            "financeiro.titulo_da_origem.v1",
            &carga(&TituloDaOrigem {
                origem_modulo: "os".to_string(),
                origem_id: Id::novo(),
            }),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(nada.is_none());
}
