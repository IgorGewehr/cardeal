//! Cotação e pedido de compra, contra SQLite de verdade: o pedido aprovado recebe pelas notas
//! (ligadas pelo `xPed` do XML ou pelo único pedido em aberto do fornecedor), a conferência
//! aponta o que diverge, o estorno desfaz o recebimento e a cotação decidida vira um pedido por
//! fornecedor.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Sessao};
use cardeal_kernel::{CodigoErro, Data, Dinheiro, Erro, Fuso, Id, Instante, Preco, Quantidade};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_clientes::{
    ConstrutorPessoa, DocumentoPessoa, Papel, RepositorioClientes, TipoDocumento, TipoPessoa,
};
use mod_compras::cotacao::Comparativo;
use mod_compras::pedido::{ConferenciaDoPedido, ResumoPedido};
use mod_compras::{
    ACaminho, AprovarPedidoCompra, CancelarPedidoCompra, ComparativoDaCotacao,
    ConferenciaComPedido, ConfirmarEntrada, Cotacao, CotacaoCompleta, DecidirCotacao,
    DefinirPreferenciasCompras, EncerrarPedidoCompra, EntradaConfirmada, EntradaFoiEstornada,
    EscolhaDaCotacao, EstadoCotacao, EstadoNotaEntrada, EstadoPedido, EstornarEntrada,
    ImportarNotaDeArquivoXml, ItemDaCotacao, ItemDoPedido, ItensDaNota, ModuloCompras,
    PedidoCompra, PedidoDeCompra, PedidoSalvo, PedidosDeCompra, RateioPor,
    RegistrarPropostaFornecedor, RelatorioImportacao, SalvarCotacao, SalvarPedidoCompra,
    UltimosPrecosPagos, VincularNotaAoPedido, VincularProdutoManual,
};
use mod_estoque::{ModuloEstoque, RepositorioEstoque};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tempfile::TempDir;

const CNPJ_EMPRESA: &str = "11222333000181";
const CNPJ_ALFA: &str = "45543915000181";
const CNPJ_BETA: &str = "11444777000161";
const GTIN: &str = "7891000315507";

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
            &ModuloCompras,
        ])
        .unwrap();
        let mut papel = cardeal_auth::Papel::novo(empresa, "Comprador");
        for p in [
            "compras.entrada.ver",
            "compras.entrada.importar",
            "compras.entrada.conferir",
            "compras.entrada.confirmar",
            "compras.entrada.estornar",
            "compras.entrada.preferencias",
            "compras.pedido.ver",
            "compras.pedido.criar",
            "compras.pedido.aprovar",
            "compras.cotacao.ver",
            "compras.cotacao.criar",
            "compras.cotacao.editar",
            "compras.cotacao.decidir",
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
        let amb = Ambiente::novo(empresa, efetivo());
        Self {
            _dir: dir,
            arm,
            empresa,
            d,
            s,
            amb,
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

    /// Um produto (com GTIN, se pedido) e um local de estoque.
    fn produto(&self, nome: &'static str, gtin: Option<&'static str>) -> Id {
        let empresa = self.empresa;
        self.arm
            .escritor()
            .executar(
                ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()),
                move |uow| {
                    let existente: Option<Vec<u8>> = uow
                        .conexao()
                        .query_row(
                            "SELECT id FROM estoque_unidade WHERE sigla = 'UN'",
                            [],
                            |r| r.get(0),
                        )
                        .ok();
                    let mut repo = RepositorioEstoque::novo(uow);
                    let grupo = Id::novo();
                    repo.inserir_grupo_produto(empresa, grupo, nome, "Geral", None)
                        .map_err(ErroArmazenamento::Dominio)?;
                    let unidade = if let Some(b) = existente {
                        Id::de_bytes(b.try_into().unwrap())
                    } else {
                        let unidade = Id::novo();
                        repo.inserir_unidade(&mod_estoque::Unidade {
                            id: unidade,
                            empresa,
                            sigla: "UN".into(),
                            nome: "Unidade".into(),
                            fracionavel: false,
                        })
                        .map_err(ErroArmazenamento::Dominio)?;
                        unidade
                    };
                    let mut produto =
                        mod_estoque::Produto::novo(empresa, grupo, nome, "22021000", unidade)
                            .map_err(|e| ErroArmazenamento::Dominio(Erro::de_dominio(&e)))?;
                    if let Some(g) = gtin {
                        produto = produto
                            .com_codigo_barras(g)
                            .map_err(|e| ErroArmazenamento::Dominio(Erro::de_dominio(&e)))?;
                    }
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
                        .inserir_local(empresa, local, "Depósito", "Deposito")
                        .map_err(ErroArmazenamento::Dominio)?;
                    Ok(local)
                },
            )
            .unwrap()
            .valor
    }

    /// Um fornecedor com o CNPJ (a importação da nota o encontra pelo documento).
    fn fornecedor(&self, cnpj: &'static str, nome: &'static str) -> Id {
        let empresa = self.empresa;
        self.arm
            .escritor()
            .executar(
                ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()),
                move |uow| {
                    let pessoa = ConstrutorPessoa::nova(
                        empresa,
                        TipoPessoa::Juridica,
                        nome,
                        Papel::Fornecedor,
                        Instante::EPOCA,
                        Data::de_dias(0),
                    )
                    .construir()
                    .map_err(|e| ErroArmazenamento::Dominio(Erro::de_dominio(&e)))?;
                    let doc = DocumentoPessoa::novo(empresa, pessoa.id, TipoDocumento::Cnpj, cnpj)
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

    fn salvar_pedido(&self, fornecedor: Id, itens: Vec<(Id, i64, i64)>) -> PedidoSalvo {
        self.executar(
            "compras.salvar_pedido.v1",
            &SalvarPedidoCompra {
                pedido: None,
                fornecedor,
                previsao_entrega: None,
                condicao_pagamento: "28 dias".into(),
                frete: Dinheiro::ZERO,
                observacao: String::new(),
                itens: itens
                    .into_iter()
                    .map(|(produto, q, centavos)| ItemDoPedido {
                        produto,
                        quantidade: Quantidade::unidades(q),
                        preco_unitario: Preco::centavos(centavos),
                    })
                    .collect(),
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

    fn importar(&self, xml: String) -> RelatorioImportacao {
        self.executar(
            "compras.importar_nota_de_arquivo_xml.v1",
            &ImportarNotaDeArquivoXml { xml },
        )
        .unwrap()
    }

    fn vincular_item(&self, nota: Id, produto: Id) {
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
    }

    fn conferencia(&self, nota: Id) -> Option<ConferenciaDoPedido> {
        self.consultar(
            "compras.conferencia_com_pedido.v1",
            &ConferenciaComPedido { nota_entrada: nota },
        )
    }

    fn confirmar(&self, nota: Id, local: Id) -> EntradaConfirmada {
        self.executar(
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
        .unwrap()
    }

    fn a_caminho(&self) -> Vec<(Id, Quantidade)> {
        self.consultar("compras.a_caminho.v1", &ACaminho)
    }
}

fn efetivo() -> cardeal_modkit::ConjuntoEfetivo {
    let mut rm = RegistroModulos::novo();
    rm.registrar(&mod_clientes::MANIFESTO).unwrap();
    rm.registrar(&mod_estoque::MANIFESTO).unwrap();
    rm.registrar(&mod_financeiro::MANIFESTO).unwrap();
    rm.registrar(&mod_compras::MANIFESTO).unwrap();
    rm.resolver(&PedidoAtivacao::nova().com_modulo("compras"))
        .unwrap()
}

/// Uma NF-e do fornecedor com um item (`quantidade` × `preco`), com o `xPed` se houver e o
/// GTIN se houver.
fn xml(chave: char, cnpj: &str, quantidade: u32, preco: &str, xped: Option<&str>) -> String {
    let chave: String = std::iter::repeat_n(chave, 44).collect();
    let q = f64::from(quantidade);
    let p: f64 = preco.parse().unwrap();
    let total = format!("{:.2}", q * p);
    let compra = xped.map_or_else(String::new, |x| {
        format!("<compra><xPed>{x}</xPed></compra>")
    });
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<nfeProc xmlns="http://www.portalfiscal.inf.br/nfe"><NFe>
<infNFe Id="NFe{chave}" versao="4.00">
  <ide><nNF>{quantidade}</nNF><serie>1</serie><dhEmi>2026-10-20T09:00:00-03:00</dhEmi></ide>
  <emit><CNPJ>{cnpj}</CNPJ><xNome>Fornecedor LTDA</xNome></emit>
  <det nItem="1"><prod><cProd>REF-1</cProd><cEAN>{GTIN}</cEAN><xProd>REFRIG COLA PET 2L</xProd>
    <NCM>22021000</NCM><qCom>{quantidade}.0000</qCom><vUnCom>{preco}</vUnCom><vProd>{total}</vProd></prod></det>
  <total><ICMSTot><vProd>{total}</vProd><vFrete>0.00</vFrete><vSeg>0.00</vSeg><vOutro>0.00</vOutro>
    <vNF>{total}</vNF></ICMSTot></total>
  {compra}
</infNFe></NFe></nfeProc>"#
    )
}

#[test]
fn o_pedido_aprovado_recebe_pelas_notas_e_o_estorno_desfaz() {
    let b = Base::nova();
    let produto = b.produto("Refrigerante Cola 2L", None);
    let local = b.local();
    let alfa = b.fornecedor(CNPJ_ALFA, "Distribuidora Alfa");
    let salvo = b.salvar_pedido(alfa, vec![(produto, 10, 400)]);
    assert_eq!((salvo.numero, salvo.estado), (1, EstadoPedido::Rascunho));
    assert!(b.a_caminho().is_empty(), "rascunho não está a caminho");
    b.aprovar(salvo.pedido);
    assert_eq!(b.a_caminho(), vec![(produto, Quantidade::unidades(10))]);

    // 1ª nota: 4 unidades, com o número do pedido no XML — liga sozinha; o item ainda não casou.
    let n1 = b.importar(xml('1', CNPJ_ALFA, 4, "4.00", Some("PC-0001")));
    let c = b.conferencia(n1.nota_entrada).expect("ligada pelo xPed");
    assert_eq!(c.numero, 1);
    assert!(c.ligacao.contains("XML"), "{}", c.ligacao);
    assert_eq!(c.sem_produto, 1);
    b.vincular_item(n1.nota_entrada, produto);
    let c = b.conferencia(n1.nota_entrada).unwrap();
    assert!(c.divergencias.is_empty(), "{:?}", c.divergencias);
    assert_eq!(c.linhas[0].faltando(), Quantidade::unidades(6));
    b.confirmar(n1.nota_entrada, local);
    let p = b.pedido(salvo.pedido);
    assert_eq!(p.estado, EstadoPedido::ParcialmenteRecebido);
    assert_eq!(p.itens[0].recebida, Quantidade::unidades(4));
    assert_eq!(p.notas.len(), 1);
    assert_eq!(p.fornecedor_nome, "Distribuidora Alfa");
    assert_eq!(b.a_caminho(), vec![(produto, Quantidade::unidades(6))]);

    // 2ª nota: o resto, sem xPed — o item casa pela regra aprendida e liga ao único pedido.
    let n2 = b.importar(xml('2', CNPJ_ALFA, 6, "4.00", None));
    let c = b
        .conferencia(n2.nota_entrada)
        .expect("único pedido em aberto");
    assert!(c.ligacao.contains("único"), "{}", c.ligacao);
    assert!(c.completa_o_pedido);
    b.confirmar(n2.nota_entrada, local);
    assert_eq!(b.pedido(salvo.pedido).estado, EstadoPedido::Recebido);
    assert!(b.a_caminho().is_empty());
    let lista: Vec<ResumoPedido> = b.consultar(
        "compras.pedidos.v1",
        &PedidosDeCompra {
            so_em_andamento: true,
            fornecedor: None,
        },
    );
    assert!(lista.is_empty(), "recebido não está em andamento");

    // O estorno da 2ª nota tira o que ela deu.
    let _: EntradaFoiEstornada = b
        .executar(
            "compras.estornar_entrada.v1",
            &EstornarEntrada {
                nota_entrada: n2.nota_entrada,
                motivo: "Lançada no mês errado".into(),
                estornar_pagamento: false,
            },
        )
        .unwrap();
    let p = b.pedido(salvo.pedido);
    assert_eq!(p.estado, EstadoPedido::ParcialmenteRecebido);
    assert_eq!(p.itens[0].recebida, Quantidade::unidades(4));

    // O último preço pago vira referência.
    let precos: Vec<mod_compras::pedido::PrecoPago> = b.consultar(
        "compras.ultimos_precos_pagos.v1",
        &UltimosPrecosPagos {
            produtos: vec![produto],
        },
    );
    assert_eq!(precos[0].preco, Preco::centavos(400));
    assert_eq!(precos[0].fornecedor, alfa);

    // O saldo encerra (não se cancela o que já recebeu).
    let r: Result<(), Erro> = b.executar(
        "compras.cancelar_pedido.v1",
        &CancelarPedidoCompra {
            pedido: salvo.pedido,
            motivo: "Não precisa mais".into(),
        },
    );
    assert_eq!(r.unwrap_err().codigo, CodigoErro::REGRA_VIOLADA);
    let (): () = b
        .executar(
            "compras.encerrar_pedido.v1",
            &EncerrarPedidoCompra {
                pedido: salvo.pedido,
                motivo: "O fornecedor não tem mais o resto".into(),
            },
        )
        .unwrap();
    assert_eq!(b.pedido(salvo.pedido).estado, EstadoPedido::Encerrado);
    assert!(
        b.a_caminho().is_empty(),
        "o encerrado não está mais a caminho"
    );
}

#[test]
fn a_nota_que_diverge_do_pedido_nao_se_confirma_sozinha() {
    let b = Base::nova();
    let produto = b.produto("Refrigerante Cola 2L", Some(GTIN));
    let local = b.local();
    let alfa = b.fornecedor(CNPJ_ALFA, "Distribuidora Alfa");
    let (): () = b
        .executar(
            "compras.definir_preferencias.v1",
            &DefinirPreferenciasCompras {
                confirma_automaticamente_quando_tudo_casa: true,
                gera_titulo_a_pagar: true,
                rateio_por: RateioPor::Valor,
                local_padrao: Some(local),
                pago_no_ato_padrao: false,
            },
        )
        .unwrap();
    let salvo = b.salvar_pedido(alfa, vec![(produto, 10, 400)]);
    b.aprovar(salvo.pedido);

    // Casa pelo GTIN, mas cobra 4,50 em vez de 4,00: fica para a conferência.
    let n1 = b.importar(xml('3', CNPJ_ALFA, 10, "4.50", Some("1")));
    assert_eq!(n1.estado, EstadoNotaEntrada::AConferir);
    let c = b.conferencia(n1.nota_entrada).unwrap();
    assert_eq!(c.divergencias.len(), 1, "{:?}", c.divergencias);
    assert!(c.divergencias[0].contains("4,50"), "{}", c.divergencias[0]);

    // Desligada do pedido, a nota não tem conferência; ligada de novo, volta.
    let (): () = b
        .executar(
            "compras.vincular_nota_ao_pedido.v1",
            &VincularNotaAoPedido {
                nota_entrada: n1.nota_entrada,
                pedido: None,
            },
        )
        .unwrap();
    assert!(b.conferencia(n1.nota_entrada).is_none());
    let (): () = b
        .executar(
            "compras.vincular_nota_ao_pedido.v1",
            &VincularNotaAoPedido {
                nota_entrada: n1.nota_entrada,
                pedido: Some(salvo.pedido),
            },
        )
        .unwrap();
    assert!(b
        .conferencia(n1.nota_entrada)
        .unwrap()
        .ligacao
        .contains("conferência"));

    // A nota no preço confirma sozinha e recebe.
    let n2 = b.importar(xml('4', CNPJ_ALFA, 10, "4.00", Some("1")));
    assert_eq!(n2.estado, EstadoNotaEntrada::Confirmada);
    assert_eq!(b.pedido(salvo.pedido).estado, EstadoPedido::Recebido);

    // A nota de outro fornecedor não se liga ao pedido da Alfa.
    b.fornecedor(CNPJ_BETA, "Atacado Beta");
    let n3 = b.importar(xml('5', CNPJ_BETA, 1, "4.00", Some("1")));
    assert!(b.conferencia(n3.nota_entrada).is_none());
    let r: Result<(), Erro> = b.executar(
        "compras.vincular_nota_ao_pedido.v1",
        &VincularNotaAoPedido {
            nota_entrada: n3.nota_entrada,
            pedido: Some(salvo.pedido),
        },
    );
    let e = r.unwrap_err();
    assert_eq!(e.codigo, CodigoErro::REGRA_VIOLADA);
    assert!(e.mensagem.contains("outro fornecedor"), "{}", e.mensagem);
}

#[test]
fn cancelar_o_pedido_solta_a_nota_que_nao_entrou() {
    let b = Base::nova();
    let produto = b.produto("Refrigerante Cola 2L", None);
    let alfa = b.fornecedor(CNPJ_ALFA, "Distribuidora Alfa");
    let salvo = b.salvar_pedido(alfa, vec![(produto, 10, 400)]);
    b.aprovar(salvo.pedido);
    let n = b.importar(xml('6', CNPJ_ALFA, 10, "4.00", Some("1")));
    assert!(b.conferencia(n.nota_entrada).is_some());
    let (): () = b
        .executar(
            "compras.cancelar_pedido.v1",
            &CancelarPedidoCompra {
                pedido: salvo.pedido,
                motivo: "Comprei de outro fornecedor".into(),
            },
        )
        .unwrap();
    assert!(b.conferencia(n.nota_entrada).is_none());
    assert_eq!(b.pedido(salvo.pedido).estado, EstadoPedido::Cancelado);

    // Editar o aprovado devolve ao rascunho; o produto repetido é recusado.
    let outro = b.salvar_pedido(alfa, vec![(produto, 2, 400)]);
    assert_eq!(outro.numero, 2);
    b.aprovar(outro.pedido);
    let r: Result<PedidoSalvo, Erro> = b.executar(
        "compras.salvar_pedido.v1",
        &SalvarPedidoCompra {
            pedido: Some(outro.pedido),
            fornecedor: alfa,
            previsao_entrega: None,
            condicao_pagamento: String::new(),
            frete: Dinheiro::reais(5),
            observacao: String::new(),
            itens: vec![
                ItemDoPedido {
                    produto,
                    quantidade: Quantidade::unidades(1),
                    preco_unitario: Preco::centavos(400),
                },
                ItemDoPedido {
                    produto,
                    quantidade: Quantidade::unidades(1),
                    preco_unitario: Preco::centavos(400),
                },
            ],
        },
    );
    assert_eq!(r.unwrap_err().codigo, CodigoErro::ENTRADA_INVALIDA);
    let editado: PedidoSalvo = b
        .executar(
            "compras.salvar_pedido.v1",
            &SalvarPedidoCompra {
                pedido: Some(outro.pedido),
                fornecedor: alfa,
                previsao_entrega: None,
                condicao_pagamento: String::new(),
                frete: Dinheiro::reais(5),
                observacao: String::new(),
                itens: vec![ItemDoPedido {
                    produto,
                    quantidade: Quantidade::unidades(3),
                    preco_unitario: Preco::centavos(400),
                }],
            },
        )
        .unwrap();
    assert_eq!(editado.estado, EstadoPedido::Rascunho);
    let p = b.pedido(outro.pedido);
    assert_eq!(p.total(), Dinheiro::reais(17));
}

#[test]
fn a_cotacao_compara_e_decide_em_um_pedido_por_fornecedor() {
    let b = Base::nova();
    let cola = b.produto("Refrigerante Cola 2L", None);
    let agua = b.produto("Água 500 ml", None);
    let alfa = b.fornecedor(CNPJ_ALFA, "Distribuidora Alfa");
    let beta = b.fornecedor(CNPJ_BETA, "Atacado Beta");
    let salva: mod_compras::cotacao::CotacaoSalva = b
        .executar(
            "compras.salvar_cotacao.v1",
            &SalvarCotacao {
                cotacao: None,
                descricao: "Reposição de bebidas".into(),
                prazo_resposta: None,
                itens: vec![
                    ItemDaCotacao {
                        produto: cola,
                        quantidade: Quantidade::unidades(10),
                    },
                    ItemDaCotacao {
                        produto: agua,
                        quantidade: Quantidade::unidades(10),
                    },
                ],
                fornecedores: vec![alfa],
            },
        )
        .unwrap();
    let cot: Option<Cotacao> = b.consultar(
        "compras.cotacao.v1",
        &CotacaoCompleta {
            cotacao: salva.cotacao,
        },
    );
    let cot = cot.unwrap();
    let (i_cola, i_agua) = (cot.itens[0].id, cot.itens[1].id);
    let proposta = |fornecedor: Id, cola_c: i64, agua_c: i64, prazo: u16| {
        let (): () = b
            .executar(
                "compras.registrar_proposta.v1",
                &RegistrarPropostaFornecedor {
                    cotacao: salva.cotacao,
                    fornecedor,
                    precos: vec![
                        (i_cola, Preco::centavos(cola_c)),
                        (i_agua, Preco::centavos(agua_c)),
                    ],
                    prazo_entrega_dias: Some(prazo),
                    condicao_pagamento: "28 dias".into(),
                    frete: Dinheiro::reais(5),
                    observacao: String::new(),
                },
            )
            .unwrap();
    };
    proposta(alfa, 1_000, 1_000, 3);
    // A Beta não foi convidada, mas mandou proposta: entra.
    proposta(beta, 1_200, 800, 7);
    let comp: Option<Comparativo> = b.consultar(
        "compras.comparativo_da_cotacao.v1",
        &ComparativoDaCotacao {
            cotacao: salva.cotacao,
        },
    );
    let comp = comp.unwrap();
    assert_eq!(comp.colunas.len(), 2);
    assert_eq!(comp.total_sugerido, Dinheiro::reais(190));
    assert_eq!(comp.sugerida, vec![(i_cola, alfa), (i_agua, beta)]);

    let pedidos: Vec<PedidoSalvo> = b
        .executar(
            "compras.decidir_cotacao.v1",
            &DecidirCotacao {
                cotacao: salva.cotacao,
                escolhas: comp
                    .sugerida
                    .iter()
                    .map(|(item, fornecedor)| EscolhaDaCotacao {
                        item: *item,
                        fornecedor: *fornecedor,
                    })
                    .collect(),
                aprovar_pedidos: true,
            },
        )
        .unwrap();
    assert_eq!(pedidos.len(), 2);
    assert!(pedidos.iter().all(|p| p.estado == EstadoPedido::Enviado));
    let pa = b.pedido(pedidos[0].pedido);
    assert_eq!(pa.fornecedor, alfa);
    assert_eq!(pa.itens.len(), 1);
    assert_eq!(pa.itens[0].preco_unitario, Preco::centavos(1_000));
    assert_eq!(pa.frete, Dinheiro::reais(5));
    assert_eq!(pa.cotacao, Some(salva.cotacao));
    assert_eq!(
        pa.previsao_entrega,
        Some(Data::hoje(Fuso::BRASILIA).mais_dias(3))
    );
    let pb = b.pedido(pedidos[1].pedido);
    assert_eq!(pb.fornecedor, beta);
    assert_eq!(pb.itens[0].preco_unitario, Preco::centavos(800));

    let cot: Option<Cotacao> = b.consultar(
        "compras.cotacao.v1",
        &CotacaoCompleta {
            cotacao: salva.cotacao,
        },
    );
    let cot = cot.unwrap();
    assert_eq!(cot.estado, EstadoCotacao::Decidida);
    assert_eq!(cot.pedidos.len(), 2);
    assert_eq!(cot.itens[1].escolhido, Some(beta));
    let r: Result<(), Erro> = b.executar(
        "compras.registrar_proposta.v1",
        &RegistrarPropostaFornecedor {
            cotacao: salva.cotacao,
            fornecedor: alfa,
            precos: vec![],
            prazo_entrega_dias: None,
            condicao_pagamento: String::new(),
            frete: Dinheiro::ZERO,
            observacao: String::new(),
        },
    );
    assert_eq!(r.unwrap_err().codigo, CodigoErro::ESTADO_INVALIDO);
}
