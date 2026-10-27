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
    AmbienteFiscal, AnularDocumentoDaVenda, BuscarXmlsQueFaltam, CancelarNfce, ConfigurarNfce,
    DefinirAmbienteFiscal, DetalheDocumento, DocumentoFiscal, DocumentoResumo, EmitirNfceDoCupom,
    EstadoDocumento, InstalarCertificado, InutilizarNumero, ModuloFiscal, NotasDoMes,
    NotasFiscaisDoMes, PedidoDocumento, ProvedorFiscal, ReenviarDocumento, ResumoDaFilaFiscal,
    ResumoFiscal, SerieDoTerminal, SituacaoFiscal, SituacaoFiscalDaEmpresa, SituacaoNoMes,
    XmlQueFalta, TAREFA_TRANSMITIR,
};
use mod_pdv::{
    AbrirCupom, AdicionarItem, CupomAberto, EstornarVenda, FinalizarVenda, ModuloPdv,
    PagamentoInformado, VendaFoiEstornada,
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
            "pdv.venda.estornar",
            "empresa.dados.ver",
            "empresa.dados.editar",
            "fiscal.ver",
            "fiscal.emitir",
            "fiscal.cancelar",
            "fiscal.configurar",
            "vendas.pedido.criar",
            "vendas.pedido.editar",
            "vendas.pedido.confirmar",
            "vendas.pedido.faturar",
            "clientes.pessoa.editar",
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
        self.vender_no(Id::novo(), cen, produto, qtd, cliente)
    }

    /// Uma venda no terminal dado.
    fn vender_no(&self, terminal: Id, cen: &Cenario, produto: Id, qtd: i64, cliente: bool) -> Id {
        let cupom: CupomAberto = self
            .executar(
                "pdv.abrir_cupom.v1",
                &AbrirCupom {
                    sessao_caixa: cen.caixa,
                    terminal,
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
                    prazo: None,
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
        xml_assinado: None,
        xml_cancelamento: None,
        erros: vec![],
        nao_enviado: false,
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
fn notas_do_mes_vao_ao_contador_com_os_xmls() {
    let m = Mundo::novo();
    let cen = m.cenario();
    m.configurar(1);
    // Uma nota de teste em homologação, antes de ir para produção: não vai ao contador.
    m.emitir(m.vender(&cen, cen.produto, 1, false));
    assert!(m.transmitir().starts_with("1 autorizada"));
    m.rodar(
        "fiscal.definir_ambiente.v1",
        &DefinirAmbienteFiscal {
            ambiente: AmbienteFiscal::Producao,
        },
    )
    .unwrap();

    // Autorizada e cancelada: o cancelamento guarda o XML do evento.
    let a = m.emitir(m.vender(&cen, cen.produto, 1, false));
    let b = m.emitir(m.vender(&cen, cen.produto, 2, false));
    assert!(m.transmitir().starts_with("2 autorizada"));
    m.rodar(
        "fiscal.cancelar_nfce.v1",
        &CancelarNfce {
            documento: b.id,
            justificativa: "Venda registrada em duplicidade no caixa".into(),
        },
    )
    .unwrap();
    assert!(m.transmitir().contains("1 cancelada"));
    // A rejeitada que não vai sair: inutilizada, com o XML da inutilização.
    let c = m.emitir(m.vender(&cen, cen.sem_ncm, 1, false));
    m.rodar(
        "fiscal.inutilizar_numero.v1",
        &InutilizarNumero {
            documento: c.id,
            justificativa: "Produto sem NCM, venda refeita em outra nota".into(),
        },
    )
    .unwrap();
    assert!(m.transmitir().contains("1 inutilizada"));
    // A resposta se perdeu: a nota assinada fica guardada, e a consulta com ela devolve o
    // nfeProc.
    let d = m.emitir(m.vender(&cen, cen.produto, 1, false));
    m.sim
        .programar(retorno(SituacaoRetorno::Processando, "TIMEOUT"));
    m.transmitir();
    m.tentar_ja();
    assert!(m.transmitir().starts_with("1 autorizada"));
    // E uma rejeitada sem desfecho: o número é um buraco até alguém resolver.
    let e = m.emitir(m.vender(&cen, cen.sem_ncm, 1, false));
    assert_eq!(e.estado, EstadoDocumento::Rejeitada);

    let mes = Data::hoje(Fuso::BRASILIA).competencia();
    let n: NotasDoMes = m.consultar("fiscal.notas_do_mes.v1", &NotasFiscaisDoMes { mes });
    assert_eq!(n.de_homologacao, 1);
    let situacoes: Vec<(u64, SituacaoNoMes)> =
        n.notas.iter().map(|x| (x.numero, x.situacao)).collect();
    assert_eq!(
        situacoes,
        vec![
            (a.numero, SituacaoNoMes::Autorizada),
            (b.numero, SituacaoNoMes::Cancelada),
            (c.numero, SituacaoNoMes::Inutilizada),
            (d.numero, SituacaoNoMes::Autorizada),
        ]
    );
    let cancelada = &n.notas[1];
    assert!(cancelada.xml.as_deref().unwrap().starts_with("<nfeProc"));
    assert!(cancelada
        .xml_evento
        .as_deref()
        .unwrap()
        .starts_with("<procEventoNFe"));
    assert_eq!(
        cancelada.protocolo_evento.as_deref(),
        Some("135260000000001")
    );
    assert_eq!(cancelada.arquivos().len(), 2);
    assert!(n.notas[2]
        .xml_evento
        .as_deref()
        .unwrap()
        .starts_with("<procInutNFe"));
    assert_eq!(n.notas[0].data, Data::hoje(Fuso::BRASILIA));
    // Nenhum XML falta: até a nota confirmada pela consulta tem o seu.
    let confirmada = n.notas[3].xml.as_deref().unwrap();
    assert!(confirmada.starts_with("<nfeProc") && confirmada.contains("<dhEmi>"));
    assert_eq!(n.faltas(), vec![]);
    assert_eq!(n.total_autorizado(), a.total + d.total);
    let sem_desfecho: Vec<Id> = n.sem_desfecho.iter().map(|x| x.id).collect();
    assert_eq!(sem_desfecho, vec![e.id]);

    // O mês anterior não tem nada disso (nem a pendência, criada depois dele).
    let antes: NotasDoMes = m.consultar(
        "fiscal.notas_do_mes.v1",
        &NotasFiscaisDoMes {
            mes: mes.anterior(),
        },
    );
    assert_eq!(antes, NotasDoMes::default());
}

#[test]
fn o_xml_do_cancelamento_que_nao_veio_e_buscado_na_consulta() {
    let m = Mundo::novo();
    let cen = m.cenario();
    m.configurar(1);
    m.rodar(
        "fiscal.definir_ambiente.v1",
        &DefinirAmbienteFiscal {
            ambiente: AmbienteFiscal::Producao,
        },
    )
    .unwrap();
    let a = m.emitir(m.vender(&cen, cen.produto, 1, false));
    let b = m.emitir(m.vender(&cen, cen.produto, 2, false));
    assert!(m.transmitir().starts_with("2 autorizada"));
    for doc in [a.id, b.id] {
        m.rodar(
            "fiscal.cancelar_nfce.v1",
            &CancelarNfce {
                documento: doc,
                justificativa: "Venda registrada em duplicidade no caixa".into(),
            },
        )
        .unwrap();
    }
    // a: o provedor (antigo) aceitou o cancelamento sem devolver o XML; b: o cancelamento já
    // estava registrado — a resposta do primeiro pedido se perdeu.
    m.sim.programar(retorno(SituacaoRetorno::Autorizado, "135"));
    m.sim.programar(retorno(SituacaoRetorno::Rejeitado, "573"));
    assert!(m.transmitir().contains("2 cancelada"));
    let resumo: ResumoFiscal = m.consultar("fiscal.resumo_da_fila.v1", &ResumoDaFilaFiscal);
    assert_eq!(resumo.xmls_a_buscar, 2);

    // A consulta das duas: canceladas; a de `a` já traz o XML do cancelamento, a de `b` ainda
    // não (a SEFAZ não tinha) — a busca encerra assim mesmo.
    let chave = |id| m.documento(id).resumo.chave.unwrap();
    let (ca, cb) = (chave(a.id), chave(b.id));
    let evento = |c: &str, protocolo: &str| {
        format!(
            "<procEventoNFe><evento/><retEvento><infEvento><chNFe>{c}</chNFe>\
             <cStat>135</cStat><nProt>{protocolo}</nProt></infEvento></retEvento></procEventoNFe>"
        )
    };
    let cancelada = |c: &str, xml: Option<String>| RetornoSefaz {
        situacao: SituacaoRetorno::Cancelado,
        codigo: "101".into(),
        motivo: "Cancelamento de NF-e homologado".into(),
        chave: Some(c.into()),
        protocolo: Some("143".into()),
        recebido_em: None,
        xml: None,
        xml_assinado: None,
        xml_cancelamento: xml,
        erros: vec![],
        nao_enviado: false,
    };
    m.sim
        .definir_consulta(&ca, cancelada(&ca, Some(evento(&ca, "135260000000011"))));
    m.sim.definir_consulta(&cb, cancelada(&cb, None));
    let r = m.transmitir();
    assert!(r.contains("1 XML(s) recuperado(s) na SEFAZ"), "{r}");
    let mes = Data::hoje(Fuso::BRASILIA).competencia();
    let n: NotasDoMes = m.consultar("fiscal.notas_do_mes.v1", &NotasFiscaisDoMes { mes });
    assert_eq!(
        n.notas[0].xml_evento.as_deref(),
        Some(evento(&ca, "135260000000011").as_str())
    );
    assert_eq!(
        n.notas[0].protocolo_evento.as_deref(),
        Some("135260000000011")
    );
    assert_eq!(n.faltas(), vec![(XmlQueFalta::EventoDeCancelamento, 1)]);
    // A busca encerrou: nada mais na fila (e a vigia não dispara por ela).
    assert_eq!(m.transmitir(), "nada na fila");
    let resumo: ResumoFiscal = m.consultar("fiscal.resumo_da_fila.v1", &ResumoDaFilaFiscal);
    assert_eq!(resumo.xmls_a_buscar, 0);

    // O pacote do contador pede de novo, e agora a SEFAZ tem o de `b`. A mesma chave só é
    // consultada de novo depois de uma hora (a SEFAZ barra consulta repetida).
    m.sim
        .definir_consulta(&cb, cancelada(&cb, Some(evento(&cb, "135260000000012"))));
    let buscando: u32 = m
        .executar("fiscal.buscar_xmls_que_faltam.v1", &BuscarXmlsQueFaltam)
        .unwrap();
    assert_eq!(buscando, 1);
    assert_eq!(m.transmitir(), "nada na fila");
    m.tentar_ja();
    assert!(m.transmitir().contains("1 XML(s) recuperado(s)"));
    let n: NotasDoMes = m.consultar("fiscal.notas_do_mes.v1", &NotasFiscaisDoMes { mes });
    assert_eq!(n.faltas(), vec![]);
    assert_eq!(
        n.notas[1].protocolo_evento.as_deref(),
        Some("135260000000012")
    );
    // Nada mais a buscar.
    let buscando: u32 = m
        .executar("fiscal.buscar_xmls_que_faltam.v1", &BuscarXmlsQueFaltam)
        .unwrap();
    assert_eq!(buscando, 0);
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

#[test]
fn cancela_a_autorizada_e_inutiliza_o_numero_da_rejeitada() {
    let m = Mundo::novo();
    let cen = m.cenario();
    m.configurar(1);
    let cancelar = |doc: Id, j: &str| {
        m.executar::<DocumentoResumo>(
            "fiscal.cancelar_nfce.v1",
            &CancelarNfce {
                documento: doc,
                justificativa: j.into(),
            },
        )
    };

    let a = m.emitir(m.vender(&cen, cen.produto, 1, false));
    assert!(m.transmitir().starts_with("1 autorizada"));

    // Justificativa curta é recusada na hora, com o campo marcado.
    let erro = cancelar(a.id, "erro").unwrap_err();
    assert_eq!(erro.campo.as_deref(), Some("justificativa"));
    // O pedido entra na fila; outro pedido em cima é recusado.
    let pedido = cancelar(a.id, "Venda registrada em duplicidade no caixa").unwrap();
    assert_eq!(
        (pedido.estado, pedido.pedido),
        (EstadoDocumento::Autorizada, Some(PedidoDocumento::Cancelar))
    );
    assert!(cancelar(a.id, "Venda registrada em duplicidade no caixa").is_err());
    let saida = m.transmitir();
    assert!(saida.contains("1 cancelada"), "{saida}");
    let d = m.documento(a.id).resumo;
    assert_eq!((d.estado, d.pedido), (EstadoDocumento::Cancelada, None));
    // Cancelada não se cancela de novo.
    assert!(cancelar(a.id, "Venda registrada em duplicidade no caixa").is_err());

    // Fora do prazo: a SEFAZ recusa; a nota continua autorizada, sem o pedido, com o motivo.
    let b = m.emitir(m.vender(&cen, cen.produto, 1, false));
    m.transmitir();
    cancelar(b.id, "Cliente desistiu da compra depois de pagar").unwrap();
    m.sim.programar(retorno(SituacaoRetorno::Rejeitado, "501"));
    assert!(m.transmitir().contains("1 pedido(s) recusado(s)"));
    let d = m.documento(b.id).resumo;
    assert_eq!((d.estado, d.pedido), (EstadoDocumento::Autorizada, None));
    assert!(d.motivo.unwrap().contains("prazo de cancelamento"));

    // A rejeitada que não vai mais sair: inutiliza o número.
    let c = m.emitir(m.vender(&cen, cen.sem_ncm, 1, false));
    assert_eq!(c.estado, EstadoDocumento::Rejeitada);
    let inutilizar = |doc: Id| {
        m.executar::<DocumentoResumo>(
            "fiscal.inutilizar_numero.v1",
            &InutilizarNumero {
                documento: doc,
                justificativa: "Produto sem NCM, venda refeita em outra nota".into(),
            },
        )
    };
    assert!(inutilizar(b.id).is_err(), "autorizada não se inutiliza");
    inutilizar(c.id).unwrap();
    assert!(m.transmitir().contains("1 inutilizada"));
    assert_eq!(
        m.documento(c.id).resumo.estado,
        EstadoDocumento::Inutilizada
    );
}

#[test]
fn venda_estornada_anula_a_nfce_conforme_o_estado() {
    let m = Mundo::novo();
    let cen = m.cenario();
    m.configurar(1);
    let estornar = |cupom: Id| -> Option<DocumentoResumo> {
        let _: VendaFoiEstornada = m
            .executar(
                "pdv.estornar_venda.v1",
                &EstornarVenda {
                    cupom,
                    motivo: "Cliente desistiu da compra".into(),
                    autorizado_por: Id::novo(),
                },
            )
            .unwrap();
        m.executar(
            "fiscal.anular_documento_da_venda.v1",
            &AnularDocumentoDaVenda {
                cupom,
                justificativa: "Venda estornada no PDV a pedido do cliente".into(),
            },
        )
        .unwrap()
    };
    let emissoes = || m.sim.emissoes.load(std::sync::atomic::Ordering::SeqCst);

    // Só a venda estornada anula a nota.
    let viva = m.vender(&cen, cen.produto, 1, false);
    let erro = m
        .executar::<Option<DocumentoResumo>>(
            "fiscal.anular_documento_da_venda.v1",
            &AnularDocumentoDaVenda {
                cupom: viva,
                justificativa: "Venda estornada no PDV a pedido do cliente".into(),
            },
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ESTADO_INVALIDO);
    // Estornada antes de a NFC-e existir: nada a anular, e a nota não nasce mais.
    let sem_nota = m.vender(&cen, cen.produto, 1, false);
    assert!(estornar(sem_nota).is_none());
    assert!(m
        .executar::<DocumentoResumo>(
            "fiscal.emitir_nfce_do_cupom.v1",
            &EmitirNfceDoCupom { cupom: sem_nota },
        )
        .is_err());

    // Autorizada: cancela.
    let a = m.vender(&cen, cen.produto, 1, false);
    let doc_a = m.emitir(a);
    assert!(m.transmitir().starts_with("1 autorizada"));
    let d = estornar(a).unwrap();
    assert!(d.anulada);
    assert_eq!(d.pedido, Some(PedidoDocumento::Cancelar));
    assert!(m.transmitir().contains("1 cancelada"));
    assert_eq!(
        m.documento(doc_a.id).resumo.estado,
        EstadoDocumento::Cancelada
    );

    // Nunca transmitida: inutiliza o número, sem emitir.
    let b = m.vender(&cen, cen.produto, 1, false);
    let doc_b = m.emitir(b);
    let antes = emissoes();
    assert_eq!(
        estornar(b).unwrap().pedido,
        Some(PedidoDocumento::Inutilizar)
    );
    assert!(m.transmitir().contains("1 inutilizada"));
    assert_eq!(
        m.documento(doc_b.id).resumo.estado,
        EstadoDocumento::Inutilizada
    );
    assert_eq!(emissoes(), antes, "não emitiu a nota da venda desfeita");

    // Rejeitada: inutiliza, e não se reenvia mais.
    let c = m.vender(&cen, cen.produto, 1, false);
    let doc_c = m.emitir(c);
    m.sim.programar(retorno(SituacaoRetorno::Rejeitado, "778"));
    assert!(m.transmitir().contains("1 rejeitada"));
    assert_eq!(
        estornar(c).unwrap().pedido,
        Some(PedidoDocumento::Inutilizar)
    );
    assert!(m
        .executar::<DocumentoResumo>(
            "fiscal.reenviar_documento.v1",
            &ReenviarDocumento {
                documento: doc_c.id
            },
        )
        .is_err());
    assert!(m.transmitir().contains("1 inutilizada"));
    assert_eq!(
        m.documento(doc_c.id).resumo.estado,
        EstadoDocumento::Inutilizada
    );

    // Tentada sem resposta (pode ter chegado à SEFAZ): a fila emite primeiro e cancela logo
    // depois — o prazo de 30 minutos conta da autorização.
    m.sim.simular_fora_do_ar(true);
    let e = m.vender(&cen, cen.produto, 1, false);
    let doc_e = m.emitir(e);
    m.transmitir();
    let d = estornar(e).unwrap();
    assert_eq!(
        (d.estado, d.pedido, d.anulada),
        (EstadoDocumento::Pendente, None, true)
    );
    m.sim.simular_fora_do_ar(false);
    m.tentar_ja();
    assert!(m.transmitir().starts_with("1 autorizada"));
    let d = m.documento(doc_e.id).resumo;
    assert_eq!(
        (d.estado, d.pedido),
        (EstadoDocumento::Autorizada, Some(PedidoDocumento::Cancelar))
    );
    assert!(m.transmitir().contains("1 cancelada"));

    // Cancelamento recusado (fora do prazo): a nota segue valendo e o caso fica à vista.
    let f = m.vender(&cen, cen.produto, 1, false);
    let doc_f = m.emitir(f);
    m.transmitir();
    estornar(f);
    m.sim.programar(retorno(SituacaoRetorno::Rejeitado, "501"));
    assert!(m.transmitir().contains("1 pedido(s) recusado(s)"));
    let d = m.documento(doc_f.id).resumo;
    assert_eq!(d.estado, EstadoDocumento::Autorizada);
    assert!(d.anulacao_pendente());
    assert_eq!(
        m.transmitir(),
        "nada na fila",
        "não insiste num cancelamento recusado"
    );
}

#[test]
fn sefaz_fora_a_nota_sai_em_contingencia_e_e_transmitida_quando_ela_volta() {
    let m = Mundo::novo();
    let cen = m.cenario();
    m.configurar(1);
    m.sim.simular_sefaz_fora(true);

    // A SEFAZ fora e a API no ar: a nota não chegou, então sai assinada em contingência.
    let venda = m.vender(&cen, cen.produto, 1, false);
    let doc = m.emitir(venda);
    assert!(m.transmitir().contains("1 em contingência"));
    let d = m.documento(doc.id);
    assert_eq!(d.resumo.estado, EstadoDocumento::Contingencia);
    let chave = d.resumo.chave.clone().unwrap();
    assert_eq!(&chave[34..35], "9", "tpEmis 9 na chave");
    assert!(d.resumo.contingencia_em.is_some());
    assert!(d.resumo.prazo_da_contingencia().is_some());
    let xml = d
        .xml_assinado
        .clone()
        .expect("a nota assinada, para o DANFCE");
    assert!(xml.contains("<tpEmis>9</tpEmis>") && xml.contains("<qrCode>"));
    assert!(xml.contains("<xJust>SEFAZ indisponível para autorização"));
    assert_eq!(d.resumo.numero, doc.numero, "o mesmo número");
    let resumo: ResumoFiscal = m.consultar("fiscal.resumo_da_fila.v1", &ResumoDaFilaFiscal);
    assert_eq!(resumo.em_contingencia, 1);
    assert!(resumo.frase().starts_with("1 nota(s) em contingência"));

    // A venda é desfeita enquanto isso: a nota já está com o consumidor — transmite e cancela.
    let _: mod_pdv::VendaFoiEstornada = m
        .executar(
            "pdv.estornar_venda.v1",
            &mod_pdv::EstornarVenda {
                cupom: venda,
                motivo: "Cliente desistiu da compra".into(),
                autorizado_por: Id::novo(),
            },
        )
        .unwrap();
    let anulada: Option<DocumentoResumo> = m
        .executar(
            "fiscal.anular_documento_da_venda.v1",
            &AnularDocumentoDaVenda {
                cupom: venda,
                justificativa: "Venda estornada no PDV a pedido do cliente".into(),
            },
        )
        .unwrap();
    assert_eq!(
        anulada.unwrap().pedido,
        None,
        "espera a contingência se resolver"
    );

    // Outra venda, ainda com a SEFAZ fora; a primeira continua esperando.
    let outra = m.emitir(m.vender(&cen, cen.produto, 2, false));
    m.tentar_ja();
    m.transmitir();
    assert_eq!(
        m.documento(outra.id).resumo.estado,
        EstadoDocumento::Contingencia
    );
    assert_eq!(
        m.documento(doc.id).resumo.estado,
        EstadoDocumento::Contingencia
    );
    assert_eq!(
        m.sim
            .contingencias
            .load(std::sync::atomic::Ordering::SeqCst),
        2
    );

    // A SEFAZ volta: o mesmo XML é transmitido e autorizado, com a mesma chave; a anulada vai
    // ao cancelamento.
    m.sim.simular_sefaz_fora(false);
    m.tentar_ja();
    assert!(m.transmitir().starts_with("2 autorizada"));
    let d = m.documento(doc.id);
    assert_eq!(d.resumo.chave.as_deref(), Some(chave.as_str()));
    assert!(d.xml.unwrap().starts_with("<nfeProc"));
    assert!(
        d.xml_assinado.is_none(),
        "o nfeProc substitui a nota assinada"
    );
    assert_eq!(d.resumo.pedido, Some(PedidoDocumento::Cancelar));
    m.tentar_ja();
    assert!(m.transmitir().contains("1 cancelada"));
    assert_eq!(
        m.documento(outra.id).resumo.estado,
        EstadoDocumento::Autorizada
    );

    // A API fora do ar (não só a SEFAZ): sem quem assine, a nota espera na fila.
    m.sim.simular_fora_do_ar(true);
    let sem_api = m.emitir(m.vender(&cen, cen.produto, 1, false));
    m.transmitir();
    assert_eq!(
        m.documento(sem_api.id).resumo.estado,
        EstadoDocumento::Pendente
    );
}

#[test]
fn cada_caixa_numera_na_sua_serie() {
    use mod_fiscal::{DefinirSerieDoTerminal, SeriesDosTerminais};
    let m = Mundo::novo();
    let cen = m.cenario();
    m.configurar(1);
    let (caixa1, caixa2) = (Id::novo(), Id::novo());
    let definir = |terminal: Id, nome: &str, serie: Option<u32>, proximo: Option<u64>| {
        m.executar::<Vec<SerieDoTerminal>>(
            "fiscal.definir_serie_do_terminal.v1",
            &DefinirSerieDoTerminal {
                terminal,
                nome: nome.into(),
                serie,
                proximo_numero: proximo,
            },
        )
    };
    // A série da empresa (1) é a dos caixas sem série própria: não vira de um terminal.
    let erro = definir(caixa2, "Caixa 2", Some(1), None).unwrap_err();
    assert_eq!(erro.campo.as_deref(), Some("serie"));
    assert!(
        erro.mensagem.contains("é a da empresa"),
        "{}",
        erro.mensagem
    );
    // O caixa 2 na série 2, começando do 100 (a série já tinha sido usada em outro sistema).
    let series = definir(caixa2, " Caixa 2 ", Some(2), Some(100)).unwrap();
    assert_eq!(
        series,
        vec![SerieDoTerminal {
            terminal: caixa2,
            nome: "Caixa 2".into(),
            serie: 2,
            proximo_numero: 100,
        }]
    );
    // A série é de um terminal só; e a da empresa não pode virar a de um terminal.
    let erro = definir(Id::novo(), "Caixa 3", Some(2), None).unwrap_err();
    assert!(
        erro.mensagem.contains("já é do Caixa 2"),
        "{}",
        erro.mensagem
    );
    let erro = m
        .executar::<mod_fiscal::ConfiguracaoNfce>(
            "fiscal.configurar_nfce.v1",
            &ConfigurarNfce {
                emitir_nas_vendas: true,
                serie: 2,
                proximo_numero: 1,
                csc_id: "000001".into(),
                csc_token: None,
            },
        )
        .unwrap_err();
    assert!(
        erro.mensagem.contains("é a do Caixa 2"),
        "{}",
        erro.mensagem
    );

    // As vendas: o caixa 2 na série 2 (100, 101); o caixa 1, sem série própria, na da empresa.
    let nota = |terminal| m.emitir(m.vender_no(terminal, &cen, cen.produto, 1, false));
    let a = nota(caixa2);
    let b = nota(caixa1);
    let c = nota(caixa2);
    assert_eq!((a.serie, a.numero), (2, 100));
    assert_eq!((b.serie, b.numero), (1, 1));
    assert_eq!((c.serie, c.numero), (2, 101));
    assert!(m.transmitir().starts_with("3 autorizada"));
    let series: Vec<SerieDoTerminal> =
        m.consultar("fiscal.series_dos_terminais.v1", &SeriesDosTerminais);
    assert_eq!(series[0].proximo_numero, 102);

    // Sem série própria de novo: volta à da empresa.
    assert!(definir(caixa2, "", None, None).unwrap().is_empty());
    let d = nota(caixa2);
    assert_eq!((d.serie, d.numero), (1, 2));
}

#[test]
fn a_nota_sem_resposta_ganha_substituta_em_contingencia() {
    use EstadoDocumento::{Autorizada, Cancelada, Contingencia, Incerta, Inutilizada};
    let m = Mundo::novo();
    let cen = m.cenario();
    m.configurar(1);
    let rodadas = |n: usize| {
        for _ in 0..n {
            m.tentar_ja();
            m.transmitir();
        }
    };
    let da_venda = |cupom: Id| -> Option<DocumentoResumo> {
        m.consultar(
            "fiscal.documento_da_venda.v1",
            &mod_fiscal::DocumentoDaVenda { cupom },
        )
    };

    // A resposta da emissão se perde (o simulador lembra da nota como autorizada).
    let venda = m.vender(&cen, cen.produto, 1, false);
    let a = m.emitir(venda);
    m.sim.programar(retorno(SituacaoRetorno::Erro, "TIMEOUT"));
    m.transmitir();
    assert_eq!(m.documento(a.id).resumo.estado, Incerta);
    // A SEFAZ cai: a consulta não responde, e o cliente ainda está no balcão — a substituta.
    m.sim.simular_sefaz_fora(true);
    m.tentar_ja();
    assert!(
        m.transmitir().contains("1 sem resposta substituída"),
        "a consulta sem resultado pede a substituta"
    );
    let b_id = m
        .documento(a.id)
        .resumo
        .substituida_por
        .expect("a substituta");
    assert_eq!(
        da_venda(venda).unwrap().id,
        b_id,
        "a nota da venda agora é a substituta"
    );
    rodadas(1);
    let b = m.documento(b_id).resumo;
    assert_eq!((b.estado, b.substitui), (Contingencia, Some(a.id)));
    assert_eq!((b.numero, b.total), (a.numero + 1, a.total));
    assert_eq!(&b.chave.unwrap()[34..35], "9");
    // Uma substituta só, por mais que a consulta siga sem resposta.
    rodadas(2);
    assert_eq!(
        m.sim
            .contingencias
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );

    // A SEFAZ volta: a substituta é autorizada; a original, achada autorizada, é cancelada por
    // substituição — depois de a substituta estar autorizada.
    m.sim.simular_sefaz_fora(false);
    rodadas(3);
    assert_eq!(m.documento(b_id).resumo.estado, Autorizada);
    let a_fim = m.documento(a.id).resumo;
    assert_eq!(a_fim.estado, Cancelada, "{:?}", a_fim.motivo);
    assert_eq!(a_fim.pedido, None);

    // Outra: a original não chegou à SEFAZ (217). Não é emitida de novo; o número é inutilizado.
    let venda = m.vender(&cen, cen.produto, 1, false);
    let c = m.emitir(venda);
    m.sim.programar(retorno(SituacaoRetorno::Erro, "TIMEOUT"));
    m.transmitir();
    let chave_c = m.documento(c.id).resumo.chave.unwrap();
    m.sim.simular_sefaz_fora(true);
    rodadas(2);
    let d_id = m
        .documento(c.id)
        .resumo
        .substituida_por
        .expect("a substituta");
    let mut nao_consta = retorno(SituacaoRetorno::Rejeitado, "217");
    nao_consta.chave = Some(chave_c.clone());
    m.sim.definir_consulta(&chave_c, nao_consta);
    let emissoes = m.sim.emissoes.load(std::sync::atomic::Ordering::SeqCst);
    m.sim.simular_sefaz_fora(false);
    rodadas(3);
    assert_eq!(m.documento(d_id).resumo.estado, Autorizada);
    let c_fim = m.documento(c.id).resumo;
    assert_eq!(c_fim.estado, Inutilizada, "{:?}", c_fim.motivo);
    assert_eq!(
        m.sim.emissoes.load(std::sync::atomic::Ordering::SeqCst),
        emissoes,
        "a original não foi emitida de novo"
    );
}

#[test]
fn o_pedido_faturado_vira_nfe_com_destinatario_e_duplicatas() {
    use mod_fiscal::{ConfiguracaoNfe, ConfigurarNfe, DocumentoDoPedido, EmitirNfeDoPedido};
    use mod_vendas::{
        AdicionarItemPedido, ConfirmarPedido, CriarPedido, FaturarPedido, ItemFoiAdicionado,
        PedidoCriado, PedidoFoiFaturado,
    };
    let m = Mundo::novo();
    let cen = m.cenario();
    m.configurar(1);
    // A NF-e numera na sua série, separada da NFC-e.
    let cfg: ConfiguracaoNfe = m
        .executar(
            "fiscal.configurar_nfe.v1",
            &ConfigurarNfe {
                serie: 2,
                proximo_numero: 50,
            },
        )
        .unwrap();
    assert_eq!((cfg.serie, cfg.proximo_numero), (2, 50));
    let pedido_faturado = |a_vista: bool| -> Id {
        let p: PedidoCriado = m
            .executar(
                "vendas.criar_pedido.v1",
                &CriarPedido {
                    cliente: cen.cliente,
                    vendedor: Id::novo(),
                    condicao_pagamento: Id::novo(),
                    tabela_preco: cen.tabela,
                    local_expedicao: cen.local,
                },
            )
            .unwrap();
        let _: ItemFoiAdicionado = m
            .executar(
                "vendas.adicionar_item_pedido.v1",
                &AdicionarItemPedido {
                    pedido: p.pedido,
                    produto: cen.produto,
                    variacao: None,
                    quantidade: Quantidade::unidades(2),
                    desconto_percentual: Percentual::ZERO,
                },
            )
            .unwrap();
        m.rodar(
            "vendas.confirmar_pedido.v1",
            &ConfirmarPedido { pedido: p.pedido },
        )
        .unwrap();
        let _: PedidoFoiFaturado = m
            .executar(
                "vendas.faturar_pedido.v1",
                &FaturarPedido {
                    pedido: p.pedido,
                    a_vista,
                },
            )
            .unwrap();
        p.pedido
    };
    let emitir = |pedido: Id| -> DocumentoResumo {
        m.executar(
            "fiscal.emitir_nfe_do_pedido.v1",
            &EmitirNfeDoPedido { pedido },
        )
        .unwrap()
    };

    // O cliente sem endereço: a nota nasce rejeitada, com o que corrigir.
    let sem_endereco = pedido_faturado(false);
    let d = emitir(sem_endereco);
    assert_eq!(d.estado, EstadoDocumento::Rejeitada);
    assert!(d.motivo.unwrap().contains("sem endereço"));

    // Com o endereço (em Caxias do Sul — o código IBGE sai pelo nome), a prazo.
    m.rodar(
        "clientes.adicionar_endereco.v1",
        &mod_clientes::AdicionarEndereco {
            pessoa: cen.cliente,
            tipo: mod_clientes::TipoEndereco::Residencial,
            logradouro: "Rua Sinimbu".into(),
            numero: "100".into(),
            complemento: None,
            bairro: "Centro".into(),
            cidade: "caxias do sul".into(),
            uf: "RS".into(),
            cep: "95020000".into(),
            principal: true,
        },
    )
    .unwrap();
    let a_prazo = pedido_faturado(false);
    let doc = emitir(a_prazo);
    assert_eq!(doc.estado, EstadoDocumento::Pendente, "{:?}", doc.motivo);
    assert_eq!(
        (doc.modelo, doc.serie, doc.numero),
        (mod_fiscal::ModeloFiscal::Nfe, 2, 51)
    );
    let nfe = m.documento(doc.id).nfe.expect("a NF-e guardada");
    assert_eq!(nfe.destinatario.endereco.codigo_municipio, "4305108");
    assert_eq!(nfe.destinatario.endereco.municipio, "Caxias do Sul");
    assert_eq!(
        nfe.destinatario.indicador_ie,
        cardeal_fiscal::IndicadorIe::NaoContribuinte
    );
    assert!(nfe.consumidor_final && nfe.a_prazo);
    assert_eq!(nfe.duplicatas.len(), 1);
    assert_eq!(nfe.duplicatas[0].valor, nfe.total());
    assert_eq!(nfe.itens[0].cfop, "5102", "dentro do RS");
    // Repetir devolve o mesmo documento; a fila autoriza, com a chave do modelo 55.
    assert_eq!(emitir(a_prazo).id, doc.id);
    assert!(m.transmitir().contains("autorizada"));
    let autorizada = m.documento(doc.id).resumo;
    assert_eq!(autorizada.estado, EstadoDocumento::Autorizada);
    assert_eq!(&autorizada.chave.unwrap()[20..22], "55");
    let do_pedido: Option<DocumentoResumo> = m.consultar(
        "fiscal.documento_do_pedido.v1",
        &DocumentoDoPedido { pedido: a_prazo },
    );
    assert_eq!(do_pedido.unwrap().id, doc.id);

    // À vista: sem duplicatas. E a NFC-e segue na série dela.
    let a_vista = emitir(pedido_faturado(true));
    let nfe = m.documento(a_vista.id).nfe.unwrap();
    assert!(!nfe.a_prazo && nfe.duplicatas.is_empty());
    assert_eq!(a_vista.numero, 52);
    let nfce = m.emitir(m.vender(&cen, cen.produto, 1, false));
    assert_eq!(
        (nfce.modelo, nfce.numero),
        (mod_fiscal::ModeloFiscal::Nfce, 1)
    );

    // A rejeitada se reenvia depois de corrigir o cadastro: mesmo número, agora pendente.
    let de_novo: DocumentoResumo = m
        .executar(
            "fiscal.reenviar_documento.v1",
            &ReenviarDocumento { documento: d.id },
        )
        .unwrap();
    assert_eq!(de_novo.estado, EstadoDocumento::Pendente);
}

#[test]
fn a_nfe_autorizada_recebe_cartas_de_correcao_pela_fila() {
    use mod_fiscal::correcao::{
        CartaDeCorrecao, CartasDeCorrecao, EmitirCartaDeCorrecao, EstadoCarta,
    };
    use mod_vendas::{
        AdicionarItemPedido, ConfirmarPedido, CriarPedido, FaturarPedido, ItemFoiAdicionado,
        PedidoCriado, PedidoFoiFaturado,
    };
    let m = Mundo::novo();
    let cen = m.cenario();
    m.configurar(1);
    m.rodar(
        "fiscal.definir_ambiente.v1",
        &DefinirAmbienteFiscal {
            ambiente: AmbienteFiscal::Producao,
        },
    )
    .unwrap();
    m.rodar(
        "clientes.adicionar_endereco.v1",
        &mod_clientes::AdicionarEndereco {
            pessoa: cen.cliente,
            tipo: mod_clientes::TipoEndereco::Residencial,
            logradouro: "Rua Sinimbu".into(),
            numero: "100".into(),
            complemento: None,
            bairro: "Centro".into(),
            cidade: "Caxias do Sul".into(),
            uf: "RS".into(),
            cep: "95020000".into(),
            principal: true,
        },
    )
    .unwrap();
    let p: PedidoCriado = m
        .executar(
            "vendas.criar_pedido.v1",
            &CriarPedido {
                cliente: cen.cliente,
                vendedor: Id::novo(),
                condicao_pagamento: Id::novo(),
                tabela_preco: cen.tabela,
                local_expedicao: cen.local,
            },
        )
        .unwrap();
    let _: ItemFoiAdicionado = m
        .executar(
            "vendas.adicionar_item_pedido.v1",
            &AdicionarItemPedido {
                pedido: p.pedido,
                produto: cen.produto,
                variacao: None,
                quantidade: Quantidade::unidades(1),
                desconto_percentual: Percentual::ZERO,
            },
        )
        .unwrap();
    m.rodar(
        "vendas.confirmar_pedido.v1",
        &ConfirmarPedido { pedido: p.pedido },
    )
    .unwrap();
    let _: PedidoFoiFaturado = m
        .executar(
            "vendas.faturar_pedido.v1",
            &FaturarPedido {
                pedido: p.pedido,
                a_vista: true,
            },
        )
        .unwrap();
    let nfe: DocumentoResumo = m
        .executar(
            "fiscal.emitir_nfe_do_pedido.v1",
            &mod_fiscal::EmitirNfeDoPedido { pedido: p.pedido },
        )
        .unwrap();
    assert!(m.transmitir().contains("1 autorizada"));
    let carta = |documento: Id, texto: &str| {
        m.executar::<CartaDeCorrecao>(
            "fiscal.emitir_carta_de_correcao.v1",
            &EmitirCartaDeCorrecao {
                documento,
                texto: texto.into(),
            },
        )
    };
    let cartas = || -> Vec<CartaDeCorrecao> {
        m.consultar(
            "fiscal.cartas_de_correcao.v1",
            &CartasDeCorrecao { documento: nfe.id },
        )
    };
    // O texto curto e a NFC-e: recusados na hora.
    assert_eq!(
        carta(nfe.id, "curto").unwrap_err().campo.as_deref(),
        Some("texto")
    );
    let nfce = m.emitir(m.vender(&cen, cen.produto, 1, false));
    assert!(carta(nfce.id, "Corrige o nome do consumidor")
        .unwrap_err()
        .mensagem
        .contains("NFC-e não tem carta"));

    // A primeira: na fila, uma por vez.
    let primeira = carta(nfe.id, "Corrige o bairro do destinatário para Centro").unwrap();
    assert_eq!(
        (primeira.sequencia, primeira.estado),
        (1, EstadoCarta::Pendente)
    );
    assert!(carta(nfe.id, "Outra correção qualquer aqui")
        .unwrap_err()
        .mensagem
        .contains("esperando a SEFAZ"));
    let resumo: ResumoFiscal = m.consultar("fiscal.resumo_da_fila.v1", &ResumoDaFilaFiscal);
    assert_eq!(resumo.cartas_pendentes, 1);
    assert!(m
        .transmitir()
        .contains("1 carta(s) de correção registrada(s)"));
    let registrada = &cartas()[0];
    assert_eq!(registrada.estado, EstadoCarta::Registrada);
    assert!(registrada.protocolo.is_some() && registrada.registrada_em.is_some());

    // A segunda recusada não gasta a sequência: a terceira volta a ser a 2.
    m.sim.programar(retorno(SituacaoRetorno::Rejeitado, "501"));
    assert_eq!(
        carta(nfe.id, "Corrige o complemento do endereço")
            .unwrap()
            .sequencia,
        2
    );
    assert!(m.transmitir().contains("1 recusada(s)"));
    assert_eq!(cartas()[0].estado, EstadoCarta::Recusada);
    assert!(cartas()[0].motivo.is_some());
    assert_eq!(
        carta(nfe.id, "Corrige o complemento do endereço, de novo")
            .unwrap()
            .sequencia,
        2
    );
    m.transmitir();

    // No pacote do contador do mês: as duas registradas, pelo mês do registro.
    let mes = Data::hoje(Fuso::BRASILIA).competencia();
    let notas: NotasDoMes = m.consultar("fiscal.notas_do_mes.v1", &NotasFiscaisDoMes { mes });
    assert_eq!(notas.cartas.len(), 2);
    let (arquivo, xml) = notas.cartas[0].arquivo();
    assert!(
        arquivo.starts_with("Saidas/NF-e/CartasDeCorrecao/110110"),
        "{arquivo}"
    );
    assert!(arquivo.ends_with("01-procEventoNFe.xml"), "{arquivo}");
    assert!(xml.contains("<tpEvento>110110</tpEvento>"));
}

#[test]
fn a_devolucao_de_compra_vira_nfe_que_se_remonta_e_transmite() {
    use cardeal_fiscal::{DevolucaoDoItem, FinalidadeNfe};
    use mod_fiscal::devolucao_compra::{
        emitir_nfe_de_devolucao_comum, DadosDaDevolucao, DocumentoDaDevolucao, ItemDevolvidoNaNota,
    };
    let m = Mundo::novo();
    let cen = m.cenario();
    m.configurar(1);
    m.rodar(
        "clientes.adicionar_endereco.v1",
        &mod_clientes::AdicionarEndereco {
            pessoa: cen.cliente,
            tipo: mod_clientes::TipoEndereco::Comercial,
            logradouro: "Rua Sinimbu".into(),
            numero: "100".into(),
            complemento: None,
            bairro: "Centro".into(),
            cidade: "Caxias do Sul".into(),
            uf: "RS".into(),
            cep: "95020000".into(),
            principal: true,
        },
    )
    .unwrap();
    let devolucao = Id::novo();
    let fornecedor = cen.cliente;
    let mut dados = DadosDaDevolucao {
        chave_da_compra: None,
        numero_da_compra: "4600".into(),
        serie_da_compra: "1".into(),
        emissao_da_compra: Data::de_ymd(2026, 9, 1).unwrap(),
        motivo: "Produto com a embalagem avariada".into(),
        itens: vec![ItemDevolvidoNaNota {
            produto: cen.produto,
            ncm: String::new(),
            quantidade: Quantidade::unidades(4),
            valor_unitario: Preco::de_str("4").unwrap(),
            valor_bruto: Dinheiro::reais(16),
            desconto: Dinheiro::ZERO,
            origem: 0,
            icms: Some((Dinheiro::reais(20), Percentual::de_str("12").unwrap())),
            com_st: true,
            devolvido: DevolucaoDoItem {
                modalidade_st: Some("4".into()),
                base_st: Dinheiro::reais(28),
                aliquota_st: Some(Percentual::de_str("17").unwrap()),
                valor_st: Dinheiro::centavos(236),
                percentual_devolvido: Percentual::de_str("40").unwrap(),
                ipi_devolvido: Dinheiro::centavos(80),
                frete: Dinheiro::reais(4),
                ..DevolucaoDoItem::default()
            },
        }],
    };
    let emitir = |d: &DadosDaDevolucao| -> DocumentoResumo {
        let ctx = cardeal_modkit::Ctx::de_sessao(&m.sessao, &m.amb);
        let d = d.clone();
        m.arm
            .escritor()
            .executar(
                ContextoEscrita::novo(m.empresa, Id::novo(), Id::novo(), Id::novo()),
                move |uow| {
                    emitir_nfe_de_devolucao_comum(devolucao, fornecedor, &d, &ctx, uow)
                        .map_err(ErroArmazenamento::Dominio)
                },
            )
            .unwrap()
            .valor
    };

    // A nota de compra lançada à mão (sem chave): rejeitada já, com o porquê.
    let rejeitada = emitir(&dados);
    assert_eq!(rejeitada.estado, EstadoDocumento::Rejeitada);
    assert!(rejeitada
        .motivo
        .as_deref()
        .is_some_and(|m| m.contains("4600/1 não tem chave de acesso")));
    // Pelas Notas fiscais não se reenvia: é pela devolução.
    let reenvio: Result<DocumentoResumo, _> = m.executar(
        "fiscal.reenviar_documento.v1",
        &ReenviarDocumento {
            documento: rejeitada.id,
        },
    );
    assert!(reenvio.unwrap_err().mensagem.contains("em Compras"));

    // Com a chave, remonta no mesmo número e vai para a fila; de novo, é a mesma nota.
    dados.chave_da_compra = Some("4".repeat(44));
    let pendente = emitir(&dados);
    assert_eq!(
        (pendente.id, pendente.numero),
        (rejeitada.id, rejeitada.numero)
    );
    assert_eq!(pendente.estado, EstadoDocumento::Pendente);
    assert_eq!(emitir(&dados).id, pendente.id);
    assert!(m.transmitir().contains("1 autorizada"));
    let doc: Option<DocumentoResumo> = m.consultar(
        "fiscal.documento_da_devolucao.v1",
        &DocumentoDaDevolucao { devolucao },
    );
    assert_eq!(doc.unwrap().estado, EstadoDocumento::Autorizada);
    let nfe = m.documento(pendente.id).nfe.unwrap();
    assert_eq!(nfe.finalidade, FinalidadeNfe::Devolucao);
    assert_eq!(nfe.referencias, vec!["4".repeat(44)]);
    assert_eq!(nfe.itens[0].cfop, "5411");
    assert_eq!(nfe.total(), Dinheiro::centavos(2_316));
}
