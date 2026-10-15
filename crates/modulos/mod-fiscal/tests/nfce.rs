//! A NFC-e de ponta a ponta contra SQLite e o `Despachante` real: uma venda de verdade no PDV
//! (cliente com CPF, produto com estoque, tabela de preço, caixa aberto), o documento criado na
//! hora (sem rede), a tarefa que transmite a fila pelo provedor (simulado), e os caminhos que
//! não podem errar — rejeição e reenvio, resposta perdida conferida pela chave, API fora do ar,
//! numeração.

#![allow(clippy::result_large_err, clippy::too_many_lines)]

use std::sync::Arc;

use cardeal_auth::{
    AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao, ValorLimite,
};
use cardeal_fiscal::{FiscalSimulado, RetornoSefaz, SituacaoRetorno};
use cardeal_kernel::{
    CodigoErro, Data, Dinheiro, Erro, Fuso, Id, Instante, Percentual, Preco, Quantidade,
};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, Recursos, RegistroModulos};
use cardeal_storage::{
    Armazenamento, Cofre, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento,
};
use mod_clientes::{
    CriarPessoa, ModuloClientes, Papel, PessoaCadastrada, TipoDocumento, TipoPessoa,
};
use mod_empresa::{DadosFiscais, DefinirDadosFiscaisEmpresa, EnderecoFiscal, ModuloEmpresa};
use mod_estoque::{
    CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, GrupoProdutoCriado, LocalCriado,
    ModuloEstoque, ProdutoCriado, RegistrarEntrada, TipoLocal, UnidadeCriada,
};
use mod_financeiro::{
    AbrirCaixa, CadastrarCaixa, CaixaCadastrado, CaixaFoiAberto, ModuloFinanceiro,
};
use mod_fiscal::{
    ConfigurarNfce, DetalheDocumento, DocumentoFiscal, DocumentoResumo, EmitirNfceDoCupom,
    EstadoDocumento, InstalarCertificado, ModuloFiscal, ProvedorFiscal, ReenviarDocumento,
    ResumoDaFilaFiscal, ResumoFiscal, SituacaoFiscal, SituacaoFiscalDaEmpresa, TAREFA_TRANSMITIR,
};
use mod_pdv::{
    AbrirCupom, AdicionarItem, CupomAberto, FinalizarVenda, ModuloPdv, PagamentoInformado,
};
use mod_vendas::{CriarRegraPreco, CriarTabelaPreco, ModuloVendas, TabelaPrecoCriada, TipoTabela};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tempfile::TempDir;

struct Mundo {
    _dir: TempDir,
    arm: Armazenamento,
    desp: Despachante,
    amb: Ambiente,
    sessao: Sessao,
    sim: Arc<FiscalSimulado>,
    empresa: Id,
}

struct Cenario {
    cliente: Id,
    produto: Id,
    sem_ncm: Id,
    local: Id,
    tabela: Id,
    caixa: Id,
}

impl Mundo {
    fn novo() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let arm =
            Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("c.db"))).unwrap();
        arm.migrar(&[
            cardeal_ledger::migracoes::conjunto(),
            ModuloEmpresa.migracoes(),
            ModuloClientes.migracoes(),
            ModuloEstoque.migracoes(),
            ModuloFinanceiro.migracoes(),
            ModuloVendas.migracoes(),
            ModuloPdv.migracoes(),
            ModuloFiscal.migracoes(),
        ])
        .unwrap();
        let empresa = Id::novo();
        arm.escritor()
            .executar(ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()), move |uow| {
                uow.conexao()
                    .execute(
                        "INSERT INTO nucleo_empresa
                           (id, razao_social, nome_fantasia, cnpj, regime, endereco, perfil, criado_em)
                         VALUES (?1,'Empresa Teste LTDA','Teste','11222333000181','SimplesNacional','{}','comercio',0)",
                        [empresa.em_bytes().as_slice()],
                    )
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                semear_plano_padrao(uow, empresa).map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .unwrap();

        let mut rm = RegistroModulos::novo();
        for m in [
            &mod_empresa::MANIFESTO,
            &mod_clientes::MANIFESTO,
            &mod_estoque::MANIFESTO,
            &mod_financeiro::MANIFESTO,
            &mod_vendas::MANIFESTO,
            &mod_pdv::MANIFESTO,
            &mod_fiscal::MANIFESTO,
        ] {
            rm.registrar(m).unwrap();
        }
        let efetivo = rm
            .resolver(
                &PedidoAtivacao::nova()
                    .com_modulo("empresa")
                    .com_modulo("pdv")
                    .com_modulo("fiscal"),
            )
            .unwrap();
        let sim = Arc::new(FiscalSimulado::novo());
        let recursos = Recursos::novo()
            .com(Arc::new(Cofre::aleatorio()))
            .com(Arc::new(ProvedorFiscal::simulado(Arc::clone(&sim))));
        let amb = Ambiente::novo(empresa, efetivo).com_recursos(Arc::new(recursos));

        let mut papel = PapelAuth::novo(empresa, "Dono");
        for p in [
            "clientes.pessoa.criar",
            "estoque.produto.criar",
            "estoque.local.criar",
            "estoque.movimento.entrada",
            "vendas.tabela_preco.criar",
            "vendas.tabela_preco.editar",
            "financeiro.caixa.cadastrar",
            "financeiro.caixa.abrir",
            "pdv.venda.editar",
            "pdv.venda.finalizar",
            "empresa.dados.ver",
            "empresa.dados.editar",
            "fiscal.ver",
            "fiscal.emitir",
            "fiscal.configurar",
        ] {
            papel = papel.com_permissao(p);
        }
        papel = papel.com_limite(
            "vendas.desconto_maximo",
            ValorLimite::Percentual(Percentual::pontos(10)),
        );
        let sessao = Sessao::abrir(EmissaoSessao::padrao(
            Id::novo(),
            Id::novo(),
            Escopo::empresa_inteira(empresa),
            AutorizacoesEfetivas::consolidar([&papel]),
            Instante::EPOCA,
        ));
        let desp = Despachante::construir(&[
            &ModuloEmpresa,
            &ModuloClientes,
            &ModuloEstoque,
            &ModuloFinanceiro,
            &ModuloVendas,
            &ModuloPdv,
            &ModuloFiscal,
        ])
        .unwrap();
        Self {
            _dir: dir,
            arm,
            desp,
            amb,
            sessao,
            sim,
            empresa,
        }
    }

    fn executar<S: DeserializeOwned>(&self, nome: &str, cmd: &impl Serialize) -> Result<S, Erro> {
        let saida = self.desp.executar_comando(
            nome,
            &postcard::to_stdvec(cmd).unwrap(),
            &self.sessao,
            &self.amb,
            self.arm.escritor(),
        )?;
        Ok(postcard::from_bytes(&saida).unwrap())
    }

    /// Executa e descarta a saída.
    fn rodar(&self, nome: &str, cmd: &impl Serialize) -> Result<(), Erro> {
        self.desp
            .executar_comando(
                nome,
                &postcard::to_stdvec(cmd).unwrap(),
                &self.sessao,
                &self.amb,
                self.arm.escritor(),
            )
            .map(|_| ())
    }

    fn consultar<S: DeserializeOwned>(&self, nome: &str, q: &impl Serialize) -> S {
        let saida = self
            .desp
            .executar_consulta(
                nome,
                &postcard::to_stdvec(q).unwrap(),
                &self.sessao,
                &self.amb,
                self.arm.leitor(),
            )
            .unwrap();
        postcard::from_bytes(&saida).unwrap()
    }

    fn transmitir(&self) -> String {
        self.desp
            .executar_tarefa(
                TAREFA_TRANSMITIR,
                &self.sessao,
                &self.amb,
                self.arm.leitor(),
                self.arm.escritor(),
            )
            .unwrap()
    }

    fn documento(&self, id: Id) -> DetalheDocumento {
        self.consultar("fiscal.documento.v1", &DocumentoFiscal { documento: id })
    }

    /// Libera a próxima tentativa já (o teste não espera o backoff).
    fn tentar_ja(&self) {
        let empresa = self.empresa;
        self.arm
            .escritor()
            .executar(
                ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()),
                move |uow| {
                    uow.conexao()
                        .execute(
                            "UPDATE fiscal_documento SET proxima_tentativa = 0 WHERE empresa = ?1",
                            [empresa.em_bytes().as_slice()],
                        )
                        .map(|_| ())
                        .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
                },
            )
            .unwrap();
    }

    fn cenario(&self) -> Cenario {
        let cliente: PessoaCadastrada = self
            .executar(
                "clientes.criar_pessoa.v1",
                &CriarPessoa {
                    tipo: TipoPessoa::Fisica,
                    nome: "João Silva".into(),
                    nome_fantasia: None,
                    papel_inicial: Papel::Cliente,
                    documento_tipo: Some(TipoDocumento::Cpf),
                    documento_numero: Some("52998224725".into()),
                    data_nascimento: None,
                    endereco: None,
                    contato: None,
                },
            )
            .unwrap();
        let grupo: GrupoProdutoCriado = self
            .executar(
                "estoque.criar_grupo_produto.v1",
                &CriarGrupoProduto {
                    codigo: "GERAL".into(),
                    nome: "Geral".into(),
                    pai: None,
                },
            )
            .unwrap();
        let unidade: UnidadeCriada = self
            .executar(
                "estoque.criar_unidade.v1",
                &CriarUnidade {
                    sigla: "UN".into(),
                    nome: "Unidade".into(),
                    fracionavel: false,
                },
            )
            .unwrap();
        let produto = |nome: &str, ncm: &str| -> Id {
            let p: ProdutoCriado = self
                .executar(
                    "estoque.criar_produto.v1",
                    &CriarProduto {
                        grupo_produto: grupo.grupo_produto,
                        nome: nome.into(),
                        ncm: ncm.into(),
                        unidade_padrao: unidade.unidade,
                        codigo_barras: None,
                        detalhes_tecnicos: None,
                    },
                )
                .unwrap();
            p.produto
        };
        let capinha = produto("Capinha de celular", "39269090");
        let brinde = produto("Brinde", "");
        let local: LocalCriado = self
            .executar(
                "estoque.criar_local.v1",
                &CriarLocal {
                    nome: "Loja".into(),
                    tipo: TipoLocal::Deposito,
                },
            )
            .unwrap();
        for p in [capinha, brinde] {
            self.rodar(
                "estoque.registrar_entrada.v1",
                &RegistrarEntrada {
                    produto: p,
                    local: local.local,
                    quantidade: Quantidade::unidades(50),
                    custo_unitario: Preco::reais(10),
                },
            )
            .unwrap();
        }
        let tabela: TabelaPrecoCriada = self
            .executar(
                "vendas.criar_tabela_preco.v1",
                &CriarTabelaPreco {
                    nome: "Balcão".into(),
                    tipo: TipoTabela::Venda,
                    vigente_de: Data::hoje(Fuso::BRASILIA).mais_dias(-1),
                    vigente_ate: None,
                },
            )
            .unwrap();
        for p in [capinha, brinde] {
            self.rodar(
                "vendas.criar_regra_preco.v1",
                &CriarRegraPreco {
                    tabela_preco: tabela.tabela_preco,
                    alvo_produto: Some(p),
                    alvo_grupo: None,
                    quantidade_minima: None,
                    preco: Preco::reais(25),
                    periodo_de: None,
                    periodo_ate: None,
                },
            )
            .unwrap();
        }
        let conta_caixa: Vec<u8> = self
            .arm
            .leitor()
            .consultar(|c| {
                c.query_row(
                    "SELECT id FROM razao_conta WHERE empresa = ?1 AND papel_padrao = 'Caixa'",
                    [self.empresa.em_bytes().as_slice()],
                    |r| r.get(0),
                )
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .unwrap();
        let caixa: CaixaCadastrado = self
            .executar(
                "financeiro.cadastrar_caixa.v1",
                &CadastrarCaixa {
                    nome: "Caixa 1".into(),
                    local_operacao: None,
                    conta_razao: Id::de_bytes(conta_caixa.try_into().unwrap()),
                    permite_negativo: false,
                },
            )
            .unwrap();
        let aberto: CaixaFoiAberto = self
            .executar(
                "financeiro.abrir_caixa.v1",
                &AbrirCaixa {
                    caixa: caixa.caixa,
                    valor_abertura: Dinheiro::ZERO,
                },
            )
            .unwrap();
        Cenario {
            cliente: cliente.pessoa,
            produto: capinha,
            sem_ncm: brinde,
            local: local.local,
            tabela: tabela.tabela_preco,
            caixa: aberto.sessao,
        }
    }

    /// Uma venda finalizada de `qtd` × R$ 25 do produto, em dinheiro; devolve o cupom.
    fn vender(&self, cen: &Cenario, produto: Id, qtd: i64, cliente: bool) -> Id {
        let cupom: CupomAberto = self
            .executar(
                "pdv.abrir_cupom.v1",
                &AbrirCupom {
                    sessao_caixa: cen.caixa,
                    terminal: Id::novo(),
                    serie_fiscal: 1,
                    cliente: cliente.then_some(cen.cliente),
                    tabela_preco: cen.tabela,
                    local_expedicao: cen.local,
                },
            )
            .unwrap();
        self.rodar(
            "pdv.adicionar_item.v1",
            &AdicionarItem {
                cupom: cupom.cupom,
                produto,
                variacao: None,
                quantidade: Quantidade::unidades(qtd),
            },
        )
        .unwrap();
        self.rodar(
            "pdv.finalizar_venda.v1",
            &FinalizarVenda {
                cupom: cupom.cupom,
                pagamentos: vec![PagamentoInformado {
                    forma: mod_pdv::FormaPagamentoPdv::Dinheiro,
                    valor: Dinheiro::reais(25 * qtd),
                }],
            },
        )
        .unwrap();
        cupom.cupom
    }

    fn emitir(&self, cupom: Id) -> DocumentoResumo {
        self.executar(
            "fiscal.emitir_nfce_do_cupom.v1",
            &EmitirNfceDoCupom { cupom },
        )
        .unwrap()
    }

    /// Dados fiscais, certificado e NFC-e ligada com o próximo número `proximo`.
    fn configurar(&self, proximo: u64) {
        let _: DadosFiscais = self
            .executar(
                "empresa.definir_dados_fiscais.v1",
                &DefinirDadosFiscaisEmpresa {
                    dados: DadosFiscais {
                        inscricao_estadual: "0960000000".into(),
                        inscricao_municipal: String::new(),
                        endereco: EnderecoFiscal {
                            logradouro: "Rua A".into(),
                            numero: "1".into(),
                            complemento: String::new(),
                            bairro: "Centro".into(),
                            municipio: "Porto Alegre".into(),
                            codigo_municipio: "4314902".into(),
                            uf: "RS".into(),
                            cep: "90010000".into(),
                        },
                    },
                },
            )
            .unwrap();
        let pfx = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../cardeal-fiscal/tests/dados/aes256.pfx"),
        )
        .unwrap();
        self.rodar(
            "fiscal.instalar_certificado.v1",
            &InstalarCertificado {
                arquivo: pfx,
                senha: "cardeal-teste".into(),
            },
        )
        .unwrap();
        self.rodar(
            "fiscal.configurar_nfce.v1",
            &ConfigurarNfce {
                emitir_nas_vendas: true,
                serie: 1,
                proximo_numero: proximo,
                csc_id: "000001".into(),
                csc_token: Some("TOKEN-DE-TESTE".into()),
            },
        )
        .unwrap();
    }
}

fn retorno(situacao: SituacaoRetorno, codigo: &str) -> RetornoSefaz {
    RetornoSefaz {
        situacao,
        codigo: codigo.into(),
        motivo: format!("Rejeição: simulada {codigo}"),
        chave: None,
        protocolo: None,
        recebido_em: None,
        xml: None,
        erros: vec![],
    }
}

#[test]
fn venda_vira_nfce_autorizada_pela_fila() {
    let m = Mundo::novo();
    let cen = m.cenario();

    // Sem configuração: o documento nasce rejeitado, dizendo o que falta — a venda não para.
    let cupom = m.vender(&cen, cen.produto, 2, true);
    let doc = m.emitir(cupom);
    assert_eq!(doc.estado, EstadoDocumento::Rejeitada);
    let motivo = doc.motivo.clone().unwrap();
    assert!(
        motivo.contains("certificado")
            && motivo.contains("CSC")
            && motivo.contains("dados fiscais"),
        "{motivo}"
    );

    // Configurada (o 1 já está reservado para esta venda), o reenvio põe na fila com o mesmo
    // número.
    m.configurar(2);
    let situacao: SituacaoFiscalDaEmpresa = m.consultar("fiscal.situacao.v1", &SituacaoFiscal);
    assert!(situacao.pendencias.is_empty(), "{:?}", situacao.pendencias);
    let doc = m
        .executar::<DocumentoResumo>(
            "fiscal.reenviar_documento.v1",
            &ReenviarDocumento { documento: doc.id },
        )
        .unwrap();
    assert_eq!(
        (doc.estado, doc.numero, doc.serie),
        (EstadoDocumento::Pendente, 1, 1)
    );
    assert_eq!(doc.total, Dinheiro::reais(50));
    // Repetir a emissão devolve o mesmo documento.
    assert_eq!(m.emitir(cupom).id, doc.id);

    let resumo: ResumoFiscal = m.consultar("fiscal.resumo_da_fila.v1", &ResumoDaFilaFiscal);
    assert_eq!(resumo.pendentes, 1);

    let saida = m.transmitir();
    let detalhe = m.documento(doc.id);
    assert!(
        saida.starts_with("1 autorizada"),
        "{saida}: {:?}",
        detalhe.resumo.motivo
    );
    assert_eq!(detalhe.resumo.estado, EstadoDocumento::Autorizada);
    let chave = detalhe.resumo.chave.clone().unwrap();
    assert_eq!(chave.len(), 44);
    assert!(detalhe.xml.as_deref().unwrap().contains("<qrCode>"));
    assert!(detalhe.resumo.protocolo.is_some() && detalhe.resumo.autorizado_em.is_some());
    let nota = detalhe.nota.unwrap();
    assert_eq!(nota.itens.len(), 1);
    assert_eq!(nota.itens[0].descricao, mod_fiscal::DESCRICAO_HOMOLOGACAO);
    assert_eq!(nota.itens[0].ncm, "39269090");
    assert_eq!(nota.itens[0].impostos.icms.csosn.as_deref(), Some("102"));
    assert_eq!(nota.emitente.endereco.uf, cardeal_kernel::Uf::Rs);
    assert!(nota.consumidor.is_some(), "CPF do cliente na nota");
    let resumo: ResumoFiscal = m.consultar("fiscal.resumo_da_fila.v1", &ResumoDaFilaFiscal);
    assert_eq!((resumo.pendentes, resumo.autorizadas_hoje), (0, 1));
    assert_eq!(m.transmitir(), "nada na fila");

    // Produto sem NCM: rejeitada na hora, sem ir à SEFAZ; o reenvio sem corrigir continua.
    let doc2 = m.emitir(m.vender(&cen, cen.sem_ncm, 1, false));
    assert_eq!((doc2.estado, doc2.numero), (EstadoDocumento::Rejeitada, 2));
    assert!(doc2
        .motivo
        .unwrap()
        .contains("Item 1 (Brinde): o produto está sem NCM"));
    let doc2 = m
        .executar::<DocumentoResumo>(
            "fiscal.reenviar_documento.v1",
            &ReenviarDocumento { documento: doc2.id },
        )
        .unwrap();
    assert_eq!(doc2.estado, EstadoDocumento::Rejeitada);
    assert_eq!(m.sim.emissoes.load(std::sync::atomic::Ordering::SeqCst), 1);

    // O número 1 já foi usado: não dá para voltar a série para trás.
    let erro = m
        .rodar(
            "fiscal.configurar_nfce.v1",
            &ConfigurarNfce {
                emitir_nas_vendas: true,
                serie: 1,
                proximo_numero: 2,
                csc_id: "000001".into(),
                csc_token: None,
            },
        )
        .unwrap_err();
    assert!(erro.mensagem.contains("já foi usado"), "{}", erro.mensagem);
}

#[test]
fn rejeicao_resposta_perdida_e_api_fora_do_ar() {
    let m = Mundo::novo();
    let cen = m.cenario();
    m.configurar(1);

    // Rejeitada pela SEFAZ → reenviada → autorizada (mesmo número).
    let rejeitada = m.emitir(m.vender(&cen, cen.produto, 1, false));
    m.sim.programar(retorno(SituacaoRetorno::Rejeitado, "778"));
    assert!(m.transmitir().contains("1 rejeitada"));
    let d = m.documento(rejeitada.id).resumo;
    assert_eq!(
        (d.estado, d.codigo.as_deref()),
        (EstadoDocumento::Rejeitada, Some("778"))
    );
    assert!(d.motivo.unwrap().starts_with("o NCM de um produto"));
    let de_novo = m
        .executar::<DocumentoResumo>(
            "fiscal.reenviar_documento.v1",
            &ReenviarDocumento {
                documento: rejeitada.id,
            },
        )
        .unwrap();
    assert_eq!(de_novo.numero, rejeitada.numero);
    assert!(m.transmitir().starts_with("1 autorizada"));

    // A resposta se perdeu: fica "conferindo" e a consulta pela chave autoriza — sem emitir de
    // novo.
    let perdida = m.emitir(m.vender(&cen, cen.produto, 1, false));
    m.sim
        .programar(retorno(SituacaoRetorno::Processando, "TIMEOUT"));
    m.transmitir();
    let d = m.documento(perdida.id).resumo;
    assert_eq!(d.estado, EstadoDocumento::Incerta);
    assert!(d.chave.is_some());
    let emissoes = m.sim.emissoes.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        m.transmitir(),
        "nada na fila",
        "espera o backoff antes de conferir"
    );
    m.tentar_ja();
    assert!(m.transmitir().starts_with("1 autorizada"));
    assert_eq!(
        m.sim.emissoes.load(std::sync::atomic::Ordering::SeqCst),
        emissoes,
        "conferiu, não reemitiu"
    );
    assert_eq!(
        m.documento(perdida.id).resumo.estado,
        EstadoDocumento::Autorizada
    );

    // API fora do ar: a venda segue, a nota espera na fila e sai quando a API volta.
    m.sim.simular_fora_do_ar(true);
    let fila = m.emitir(m.vender(&cen, cen.produto, 1, false));
    assert!(m.transmitir().contains("1 na fila"));
    let d = m.documento(fila.id).resumo;
    assert_eq!((d.estado, d.tentativas), (EstadoDocumento::Pendente, 1));
    assert!(d.motivo.unwrap().contains("não respondeu"));
    let resumo: ResumoFiscal = m.consultar("fiscal.resumo_da_fila.v1", &ResumoDaFilaFiscal);
    assert_eq!(resumo.pendentes, 1);
    assert!(resumo.esperando_desde.is_some());
    m.sim.simular_fora_do_ar(false);
    m.tentar_ja();
    assert!(m.transmitir().starts_with("1 autorizada"));
    let numeros: Vec<u64> = [rejeitada.id, perdida.id, fila.id]
        .iter()
        .map(|id| m.documento(*id).resumo.numero)
        .collect();
    assert_eq!(numeros, vec![1, 2, 3]);
}

#[test]
fn transmitir_sem_permissao_e_recusado() {
    let m = Mundo::novo();
    let mut papel = PapelAuth::novo(m.empresa, "Visitante");
    papel = papel.com_permissao("fiscal.ver");
    let visitante = Sessao::abrir(EmissaoSessao::padrao(
        Id::novo(),
        Id::novo(),
        Escopo::empresa_inteira(m.empresa),
        AutorizacoesEfetivas::consolidar([&papel]),
        Instante::EPOCA,
    ));
    let erro = m
        .desp
        .executar_tarefa(
            TAREFA_TRANSMITIR,
            &visitante,
            &m.amb,
            m.arm.leitor(),
            m.arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::SEM_PERMISSAO);
}
