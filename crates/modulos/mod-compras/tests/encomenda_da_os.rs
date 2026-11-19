//! A peça que falta numa OS, comprada pelo pedido de compra, contra SQLite de verdade: o pedido
//! guarda para qual OS é cada parte do item (e marca a peça como encomendada na mesma
//! transação), a nota que entra pelo pedido **reserva** a peça para a OS (o balcão não a vende),
//! o "Aplicar" consome a reserva, e o "Chegou" da OS — que lançaria a entrada e a conta a pagar
//! de novo — é recusado para a peça que vem pela nota. Cancelar o pedido devolve a peça a "a
//! comprar"; estornar a nota tira a reserva.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Sessao};
use cardeal_kernel::{CodigoErro, Data, Dinheiro, Erro, Fuso, Id, Instante, Preco, Quantidade};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_clientes::{
    ConstrutorPessoa, ContatoInicial, CriarPessoa, DocumentoPessoa, Papel, PessoaCadastrada,
    RepositorioClientes, TipoContato, TipoDocumento, TipoPessoa,
};
use mod_compras::{
    AprovarPedidoCompra, CancelarPedidoCompra, ConfirmarEntrada, EntradaConfirmada,
    EntradaFoiEstornada, EstornarEntrada, ImportarNotaDeArquivoXml, ItemDoPedido, ItensDaNota,
    ModuloCompras, PecaDaOsNoPedido, PedidoCompra, PedidoDeCompra, PedidoSalvo,
    RelatorioImportacao, SalvarPedidoCompra, VincularProdutoManual,
};
use mod_estoque::{ModuloEstoque, RepositorioEstoque};
use mod_os::{
    AbrirOrdemServico, AplicarPeca, AprovarOrcamentoOs, BuscarDetalheOrdem, DetalheOrdem,
    EnviarParaAprovacao, IniciarExecucao, ItemOrcamentoNovo, ModuloOs, MontarOrcamentoOs,
    OrdemServicoAberta,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tempfile::TempDir;

const CNPJ_EMPRESA: &str = "11222333000181";
const CNPJ_ALFA: &str = "45543915000181";

struct Base {
    _dir: TempDir,
    arm: Armazenamento,
    empresa: Id,
    d: Despachante,
    s: Sessao,
    amb: Ambiente,
}

impl Base {
    fn nova() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let arm =
            Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("c.db"))).unwrap();
        arm.migrar(&[
            cardeal_ledger::migracoes::conjunto(),
            mod_clientes::ModuloClientes.migracoes(),
            ModuloEstoque.migracoes(),
            mod_financeiro::ModuloFinanceiro.migracoes(),
            ModuloOs.migracoes(),
            ModuloCompras.migracoes(),
        ])
        .unwrap();
        let empresa = Id::novo();
        arm.escritor()
            .executar(
                ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()),
                move |uow| {
                    uow.conexao()
                        .execute(
                            "INSERT INTO nucleo_empresa
                               (id, razao_social, nome_fantasia, cnpj, regime, endereco, perfil,
                                criado_em)
                             VALUES (?1,'Teste LTDA','Teste',?2,'SimplesNacional','{}','comercio',0)",
                            rusqlite::params![empresa.em_bytes().as_slice(), CNPJ_EMPRESA],
                        )
                        .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                    semear_plano_padrao(uow, empresa)
                        .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
                },
            )
            .unwrap();
        let d = Despachante::construir(&[
            &mod_clientes::ModuloClientes,
            &ModuloEstoque,
            &mod_financeiro::ModuloFinanceiro,
            &ModuloOs,
            &ModuloCompras,
        ])
        .unwrap();
        let mut papel = cardeal_auth::Papel::novo(empresa, "Balcão e compras");
        for p in [
            "clientes.pessoa.criar",
            "compras.entrada.ver",
            "compras.entrada.importar",
            "compras.entrada.conferir",
            "compras.entrada.confirmar",
            "compras.entrada.estornar",
            "compras.pedido.ver",
            "compras.pedido.criar",
            "compras.pedido.aprovar",
            "estoque.saldo.ver",
            "estoque.inventario.criar",
            "estoque.inventario.contar",
            "estoque.inventario.encerrar",
            "financeiro.pagar.criar",
            "financeiro.pagar.baixar",
            "financeiro.pagar.ver",
            "os.ordem.criar",
            "os.ordem.ver",
            "os.orcamento.montar",
            "os.orcamento.enviar",
            "os.orcamento.aprovar",
            "os.execucao.iniciar",
            "os.peca.aplicar",
        ] {
            papel = papel.com_permissao(p);
        }
        let s = Sessao::abrir(EmissaoSessao::padrao(
            Id::novo(),
            Id::novo(),
            Escopo::empresa_inteira(empresa),
            AutorizacoesEfetivas::consolidar([&papel]),
            Instante::EPOCA,
        ));
        let mut rm = RegistroModulos::novo();
        for m in [
            &mod_clientes::MANIFESTO,
            &mod_estoque::MANIFESTO,
            &mod_financeiro::MANIFESTO,
            &mod_os::MANIFESTO,
            &mod_compras::MANIFESTO,
        ] {
            rm.registrar(m).unwrap();
        }
        let efetivo = rm
            .resolver(
                &PedidoAtivacao::nova()
                    .com_modulo("compras")
                    .com_modulo("os"),
            )
            .unwrap();
        Self {
            _dir: dir,
            arm,
            empresa,
            d,
            s,
            amb: Ambiente::novo(empresa, efetivo),
        }
    }

    fn executar<C: Serialize, S: DeserializeOwned>(&self, nome: &str, cmd: &C) -> Result<S, Erro> {
        self.d
            .executar_comando(
                nome,
                &postcard::to_stdvec(cmd).unwrap(),
                &self.s,
                &self.amb,
                self.arm.escritor(),
            )
            .map(|b| postcard::from_bytes(&b).unwrap())
    }

    fn consultar<C: Serialize, S: DeserializeOwned>(&self, nome: &str, q: &C) -> S {
        let b = self
            .d
            .executar_consulta(
                nome,
                &postcard::to_stdvec(q).unwrap(),
                &self.s,
                &self.amb,
                self.arm.leitor(),
            )
            .unwrap();
        postcard::from_bytes(&b).unwrap()
    }

    /// Um produto sem estoque nenhum.
    fn produto(&self, nome: &'static str) -> Id {
        let empresa = self.empresa;
        self.arm
            .escritor()
            .executar(
                ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()),
                move |uow| {
                    let mut repo = RepositorioEstoque::novo(uow);
                    let grupo = Id::novo();
                    repo.inserir_grupo_produto(empresa, grupo, nome, "Peças", None)
                        .map_err(ErroArmazenamento::Dominio)?;
                    let unidade = Id::novo();
                    repo.inserir_unidade(&mod_estoque::Unidade {
                        id: unidade,
                        empresa,
                        sigla: format!("U{}", &nome[..2]),
                        nome: "Unidade".into(),
                        fracionavel: false,
                    })
                    .map_err(ErroArmazenamento::Dominio)?;
                    let produto =
                        mod_estoque::Produto::novo(empresa, grupo, nome, "85177099", unidade)
                            .map_err(|e| ErroArmazenamento::Dominio(Erro::de_dominio(&e)))?;
                    repo.inserir_produto(&produto)
                        .map_err(ErroArmazenamento::Dominio)?;
                    Ok(produto.id)
                },
            )
            .unwrap()
            .valor
    }

    fn local(&self) -> Id {
        let empresa = self.empresa;
        self.arm
            .escritor()
            .executar(
                ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()),
                move |uow| {
                    let local = Id::novo();
                    RepositorioEstoque::novo(uow)
                        .inserir_local(empresa, local, "Loja", "Loja")
                        .map_err(ErroArmazenamento::Dominio)?;
                    Ok(local)
                },
            )
            .unwrap()
            .valor
    }

    fn fornecedor(&self) -> Id {
        let empresa = self.empresa;
        self.arm
            .escritor()
            .executar(
                ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()),
                move |uow| {
                    let pessoa = ConstrutorPessoa::nova(
                        empresa,
                        TipoPessoa::Juridica,
                        "Distribuidora Alfa",
                        Papel::Fornecedor,
                        Instante::EPOCA,
                        Data::de_dias(0),
                    )
                    .construir()
                    .map_err(|e| ErroArmazenamento::Dominio(Erro::de_dominio(&e)))?;
                    let doc =
                        DocumentoPessoa::novo(empresa, pessoa.id, TipoDocumento::Cnpj, CNPJ_ALFA)
                            .map_err(|e| ErroArmazenamento::Dominio(Erro::de_dominio(&e)))?;
                    let mut repo = RepositorioClientes::novo(uow);
                    repo.inserir_pessoa(&pessoa)
                        .map_err(ErroArmazenamento::Dominio)?;
                    repo.inserir_documento(&doc)
                        .map_err(ErroArmazenamento::Dominio)?;
                    Ok(pessoa.id)
                },
            )
            .unwrap()
            .valor
    }

    fn cliente(&self) -> Id {
        let c: PessoaCadastrada = self
            .executar(
                "clientes.criar_pessoa.v1",
                &CriarPessoa {
                    tipo: TipoPessoa::Fisica,
                    nome: "Maria".to_owned(),
                    nome_fantasia: None,
                    papel_inicial: Papel::Cliente,
                    documento_tipo: None,
                    documento_numero: None,
                    data_nascimento: None,
                    endereco: None,
                    contato: Some(ContatoInicial {
                        tipo: TipoContato::Whatsapp,
                        valor: "31999998888".to_owned(),
                    }),
                },
            )
            .unwrap();
        c.pessoa
    }

    /// Uma OS aprovada e em execução com uma peça (1 un) do `produto`; devolve (OS, item).
    fn os_com_peca(&self, cliente: Id, produto: Id) -> (Id, Id) {
        let os: OrdemServicoAberta = self
            .executar(
                "os.abrir_ordem_servico.v1",
                &AbrirOrdemServico {
                    cliente,
                    equipamento: "Samsung A52".to_owned(),
                    defeito_relatado: "Tela".to_owned(),
                    tecnico_responsavel: Id::novo(),
                    garantia_dias: 90,
                    ficha: mod_os::FichaEntrada::default(),
                },
            )
            .unwrap();
        let os = os.ordem_servico;
        let item: Id = self
            .executar(
                "os.montar_orcamento.v1",
                &MontarOrcamentoOs {
                    ordem_servico: os,
                    item: ItemOrcamentoNovo::Peca {
                        produto,
                        quantidade: Quantidade::unidades(1),
                        preco_unitario: Preco::reais(320),
                    },
                },
            )
            .unwrap();
        let (): () = self
            .executar(
                "os.enviar_para_aprovacao.v1",
                &EnviarParaAprovacao { ordem_servico: os },
            )
            .unwrap();
        let (): () = self
            .executar(
                "os.aprovar_orcamento.v1",
                &AprovarOrcamentoOs {
                    ordem_servico: os,
                    identificacao_aprovador: "Cliente, no balcão".to_owned(),
                },
            )
            .unwrap();
        let (): () = self
            .executar(
                "os.iniciar_execucao.v1",
                &IniciarExecucao { ordem_servico: os },
            )
            .unwrap();
        (os, item)
    }

    fn detalhe(&self, os: Id) -> DetalheOrdem {
        let d: Option<DetalheOrdem> = self.consultar(
            "os.buscar_detalhe_ordem.v1",
            &BuscarDetalheOrdem { ordem_servico: os },
        );
        d.unwrap()
    }

    /// Pedido de `quantidade` do produto a R$ 180, com as peças das OS que ele atende.
    fn pedido_para_os(
        &self,
        fornecedor: Id,
        produto: Id,
        quantidade: i64,
        pecas: &[Id],
    ) -> PedidoSalvo {
        self.executar(
            "compras.salvar_pedido.v1",
            &SalvarPedidoCompra {
                pedido: None,
                fornecedor,
                previsao_entrega: Some(Data::hoje(Fuso::BRASILIA).mais_dias(3)),
                condicao_pagamento: "28 dias".into(),
                frete: Dinheiro::ZERO,
                observacao: String::new(),
                itens: vec![ItemDoPedido {
                    produto,
                    quantidade: Quantidade::unidades(quantidade),
                    preco_unitario: Preco::reais(180),
                    para_os: pecas
                        .iter()
                        .map(|&item_peca| PecaDaOsNoPedido {
                            item_peca,
                            quantidade: Quantidade::unidades(1),
                        })
                        .collect(),
                }],
            },
        )
        .unwrap()
    }

    fn aprovar(&self, pedido: Id) {
        let (): () = self
            .executar("compras.aprovar_pedido.v1", &AprovarPedidoCompra { pedido })
            .unwrap();
    }

    fn pedido(&self, pedido: Id) -> PedidoCompra {
        let p: Option<PedidoCompra> =
            self.consultar("compras.pedido.v1", &PedidoDeCompra { pedido });
        p.unwrap()
    }

    /// A nota do pedido chega (pelo `xPed`), com o item casado à mão, e é confirmada no local.
    fn receber_nota(&self, produto: Id, local: Id, quantidade: u32) -> Id {
        let r: RelatorioImportacao = self
            .executar(
                "compras.importar_nota_de_arquivo_xml.v1",
                &ImportarNotaDeArquivoXml {
                    xml: xml(quantidade, Some("PC-0001")),
                },
            )
            .unwrap();
        let nota = r.nota_entrada;
        let itens: Vec<mod_compras::ItemNotaEntrada> =
            self.consultar("compras.itens_da_nota.v1", &ItensDaNota { nota });
        let (): () = self
            .executar(
                "compras.vincular_produto_manual.v1",
                &VincularProdutoManual {
                    item_nota: itens[0].id,
                    produto,
                },
            )
            .unwrap();
        let _: EntradaConfirmada = self
            .executar(
                "compras.confirmar_entrada.v1",
                &ConfirmarEntrada {
                    nota_entrada: nota,
                    local,
                    gerar_titulo_a_pagar: Some(true),
                    pago_no_ato: Some(false),
                    parcelas: None,
                    incluir_st_estimado: false,
                },
            )
            .unwrap();
        nota
    }

    fn disponivel(&self, produto: Id) -> Quantidade {
        self.consultar(
            "estoque.saldo_disponivel_do_produto.v1",
            &mod_estoque::SaldoDisponivelDoProduto { produto },
        )
    }

    fn a_pagar(&self) -> Vec<mod_financeiro::ItemTituloEmAberto> {
        self.consultar(
            "financeiro.titulos_a_pagar_em_aberto.v1",
            &mod_financeiro::TitulosAPagarEmAberto,
        )
    }

    fn chegou(&self, os: Id, item_peca: Id) -> Result<mod_os::ChegadaRegistrada, Erro> {
        self.executar(
            "os.registrar_chegada_da_peca.v1",
            &mod_os::RegistrarChegadaDaPeca {
                ordem_servico: os,
                item_peca,
                custo_unitario: Preco::reais(180),
                fornecedor: String::new(),
                pagamento: mod_os::PagamentoDaPeca::APrazo {
                    vencimento: Data::hoje(Fuso::BRASILIA).mais_dias(15),
                },
            },
        )
    }

    fn aplicar(&self, os: Id, item_peca: Id, local: Id) -> Result<mod_os::PecaFoiAplicada, Erro> {
        self.executar(
            "os.aplicar_peca.v1",
            &AplicarPeca {
                ordem_servico: os,
                item_peca,
                local,
                lote: None,
            },
        )
    }
}

/// A NF-e da Distribuidora Alfa com a tela (`quantidade` × R$ 180).
fn xml(quantidade: u32, xped: Option<&str>) -> String {
    let chave: String = std::iter::repeat_n('7', 44).collect();
    let total = format!("{:.2}", f64::from(quantidade) * 180.0);
    let compra = xped.map_or_else(String::new, |x| {
        format!("<compra><xPed>{x}</xPed></compra>")
    });
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<nfeProc xmlns="http://www.portalfiscal.inf.br/nfe"><NFe>
<infNFe Id="NFe{chave}" versao="4.00">
  <ide><nNF>55</nNF><serie>1</serie><dhEmi>2026-11-10T09:00:00-03:00</dhEmi></ide>
  <emit><CNPJ>{CNPJ_ALFA}</CNPJ><xNome>Distribuidora Alfa LTDA</xNome></emit>
  <det nItem="1"><prod><cProd>TL-A52</cProd><cEAN>SEM GTIN</cEAN><xProd>TELA A52 ORIGINAL</xProd>
    <NCM>85177099</NCM><qCom>{quantidade}.0000</qCom><vUnCom>180.00</vUnCom><vProd>{total}</vProd></prod></det>
  <total><ICMSTot><vProd>{total}</vProd><vFrete>0.00</vFrete><vSeg>0.00</vSeg><vOutro>0.00</vOutro>
    <vNF>{total}</vNF></ICMSTot></total>
  {compra}
</infNFe></NFe></nfeProc>"#
    )
}

#[test]
fn a_peca_da_os_vem_pela_nota_do_pedido_fica_reservada_e_entra_uma_vez_so() {
    let b = Base::nova();
    let produto = b.produto("Tela A52");
    let local = b.local();
    let alfa = b.fornecedor();
    let cliente = b.cliente();
    let (os, item) = b.os_com_peca(cliente, produto);

    // 1. O pedido sai da OS: a peça fica encomendada, ligada ao pedido e ao fornecedor.
    let salvo = b.pedido_para_os(alfa, produto, 1, &[item]);
    let peca = &b.detalhe(os).itens_peca[0];
    let encomenda = peca.encomenda.as_ref().expect("encomendada pelo pedido");
    assert_eq!(encomenda.pedido, Some(salvo.pedido));
    assert_eq!(encomenda.fornecedor, "Distribuidora Alfa");
    assert_eq!(encomenda.custo_previsto, Some(Preco::reais(180)));
    let p = b.pedido(salvo.pedido);
    assert_eq!(p.itens[0].para_os.len(), 1, "o pedido sabe para qual OS é");

    // 2. A nota do pedido entra: a peça fica reservada para a OS, fora do disponível.
    b.aprovar(salvo.pedido);
    b.receber_nota(produto, local, 1);
    assert_eq!(
        b.disponivel(produto),
        Quantidade::ZERO,
        "reservada para a OS"
    );
    let d = b.detalhe(os);
    assert_eq!(d.reservado_para(item), Quantidade::unidades(1));
    assert_eq!(b.a_pagar().len(), 1, "a conta a pagar é a da nota");

    // 3. O "Chegou" lançaria a entrada e a conta de novo: recusado.
    let erro = b.chegou(os, item).unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::REGRA_VIOLADA, "{}", erro.mensagem);
    assert_eq!(b.a_pagar().len(), 1);

    // 4. Aplicar consome a reserva, ao custo da nota.
    let aplicada = b.aplicar(os, item, local).unwrap();
    assert_eq!(aplicada.custo_unitario, Preco::reais(180));
    assert!(!aplicada.gerou_divergencia, "a peça estava lá");
    let d = b.detalhe(os);
    assert!(d.itens_peca[0].aplicada);
    assert_eq!(d.reservado_para(item), Quantidade::ZERO);
    assert_eq!(b.disponivel(produto), Quantidade::ZERO);
}

#[test]
fn uma_linha_do_pedido_atende_duas_os_e_o_estoque() {
    let b = Base::nova();
    let produto = b.produto("Tela A52");
    let local = b.local();
    let alfa = b.fornecedor();
    let cliente = b.cliente();
    let (os1, item1) = b.os_com_peca(cliente, produto);
    let (os2, item2) = b.os_com_peca(cliente, produto);

    let salvo = b.pedido_para_os(alfa, produto, 3, &[item1, item2]);
    b.aprovar(salvo.pedido);
    b.receber_nota(produto, local, 3);

    assert_eq!(
        b.detalhe(os1).reservado_para(item1),
        Quantidade::unidades(1)
    );
    assert_eq!(
        b.detalhe(os2).reservado_para(item2),
        Quantidade::unidades(1)
    );
    assert_eq!(
        b.disponivel(produto),
        Quantidade::unidades(1),
        "a terceira é do estoque, livre para vender"
    );
}

#[test]
fn cancelar_o_pedido_devolve_a_peca_para_a_comprar() {
    let b = Base::nova();
    let produto = b.produto("Tela A52");
    let alfa = b.fornecedor();
    let cliente = b.cliente();
    let (os, item) = b.os_com_peca(cliente, produto);

    let salvo = b.pedido_para_os(alfa, produto, 1, &[item]);
    assert!(b.detalhe(os).itens_peca[0].encomenda.is_some());
    let (): () = b
        .executar(
            "compras.cancelar_pedido.v1",
            &CancelarPedidoCompra {
                pedido: salvo.pedido,
                motivo: "Fornecedor sem a peça".into(),
            },
        )
        .unwrap();
    assert!(
        b.detalhe(os).itens_peca[0].encomenda.is_none(),
        "volta para a lista de compras"
    );
}

#[test]
fn estornar_a_nota_tira_a_reserva_da_os() {
    let b = Base::nova();
    let produto = b.produto("Tela A52");
    let local = b.local();
    let alfa = b.fornecedor();
    let cliente = b.cliente();
    let (os, item) = b.os_com_peca(cliente, produto);

    let salvo = b.pedido_para_os(alfa, produto, 1, &[item]);
    b.aprovar(salvo.pedido);
    let nota = b.receber_nota(produto, local, 1);
    assert_eq!(b.detalhe(os).reservado_para(item), Quantidade::unidades(1));

    let _: EntradaFoiEstornada = b
        .executar(
            "compras.estornar_entrada.v1",
            &EstornarEntrada {
                nota_entrada: nota,
                motivo: "Nota lançada em duplicidade".into(),
                estornar_pagamento: false,
            },
        )
        .unwrap();
    let d = b.detalhe(os);
    assert_eq!(d.reservado_para(item), Quantidade::ZERO);
    assert!(
        d.itens_peca[0].encomenda.is_some(),
        "continua encomendada: o pedido volta a esperar"
    );
    assert_eq!(b.disponivel(produto), Quantidade::ZERO);
}

#[test]
fn o_inventario_conta_a_peca_reservada_sem_inventar_sobra() {
    let b = Base::nova();
    let produto = b.produto("Tela A52");
    let local = b.local();
    let alfa = b.fornecedor();
    let cliente = b.cliente();
    let (os, item) = b.os_com_peca(cliente, produto);
    let salvo = b.pedido_para_os(alfa, produto, 1, &[item]);
    b.aprovar(salvo.pedido);
    b.receber_nota(produto, local, 1);

    // A tela está na prateleira, guardada para a OS: quem conta, conta 1.
    let inv: mod_estoque::InventarioCriado = b
        .executar(
            "estoque.criar_inventario.v1",
            &mod_estoque::CriarInventario {
                local,
                grupo: None,
                descricao: "Balanço de novembro".into(),
            },
        )
        .unwrap();
    let _: mod_estoque::ContagemIniciada = b
        .executar(
            "estoque.iniciar_contagem.v1",
            &mod_estoque::IniciarContagem {
                inventario: inv.inventario,
            },
        )
        .unwrap();
    let _: mod_estoque::ContagemRegistrada = b
        .executar(
            "estoque.registrar_contagem.v1",
            &mod_estoque::RegistrarContagem {
                inventario: inv.inventario,
                produto,
                quantidade: Quantidade::unidades(1),
            },
        )
        .unwrap();
    let r: mod_estoque::InventarioEncerradoResumo = b
        .executar(
            "estoque.encerrar_inventario.v1",
            &mod_estoque::EncerrarInventario {
                inventario: inv.inventario,
            },
        )
        .unwrap();
    assert_eq!(r.ajustes, 0, "a peça reservada está no local");
    assert_eq!(b.detalhe(os).reservado_para(item), Quantidade::unidades(1));
    assert_eq!(b.disponivel(produto), Quantidade::ZERO);
}
