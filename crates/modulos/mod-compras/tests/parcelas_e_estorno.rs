//! As parcelas da nota (as duplicatas do XML, `cobr/dup`) viram as parcelas da conta a pagar, e
//! a entrada confirmada se estorna: a mercadoria sai do estoque ao custo da nota, a conta é
//! cancelada (com o pagamento, se pedido) e a nota volta para a conferência. Lições do
//! `gestao-raiz` (parcelas da nota e `revertNoteImport`), contra SQLite de verdade.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Sessao};
use cardeal_kernel::{CodigoErro, Competencia, Data, Dinheiro, Erro, Id, Instante, Quantidade};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Ctx, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_compras::{
    ConfirmarEntrada, DevolucaoAoFornecedor, DevolucaoFeita, DevolucoesDaNota,
    DevolverAoFornecedor, EntradaConfirmada, EntradaFoiEstornada, EstadoNotaEntrada,
    EstornarEntrada, ImportarNotaDeArquivoXml, ItemADevolver, ItensDaNota, ModuloCompras,
    NotaDeEntradaDoMes, NotasDeEntradaDoMes, ParcelaCompra, ParcelasDaNota,
    PreviaDoEstornoDaEntrada, PreviaEstornoEntrada, RelatorioImportacao, VincularProdutoManual,
};
use mod_estoque::{ModuloEstoque, RepositorioEstoque};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tempfile::TempDir;

const CNPJ_EMPRESA: &str = "11222333000181";
const CNPJ_FORNECEDOR: &str = "45543915000181";

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
        let mut papel = cardeal_auth::Papel::novo(empresa, "Testador");
        for p in [
            "compras.entrada.ver",
            "compras.entrada.importar",
            "compras.entrada.conferir",
            "compras.entrada.confirmar",
            "compras.entrada.estornar",
            "compras.devolucao.criar",
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

    fn ctx(&self) -> Ctx {
        let sessao = Sessao::abrir(EmissaoSessao::padrao(
            Id::novo(),
            Id::novo(),
            Escopo::empresa_inteira(self.empresa),
            AutorizacoesEfetivas::default(),
            Instante::EPOCA,
        ));
        Ctx::de_sessao(&sessao, &Ambiente::novo(self.empresa, efetivo()))
    }

    /// Um produto e um local de estoque.
    fn produto_e_local(&self) -> (Id, Id) {
        let empresa = self.empresa;
        self.arm
            .escritor()
            .executar(
                ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()),
                move |uow| {
                    let (grupo, unidade, local) = (Id::novo(), Id::novo(), Id::novo());
                    let mut repo = RepositorioEstoque::novo(uow);
                    repo.inserir_grupo_produto(empresa, grupo, "GERAL", "Geral", None)
                        .map_err(ErroArmazenamento::Dominio)?;
                    repo.inserir_unidade(&mod_estoque::Unidade {
                        id: unidade,
                        empresa,
                        sigla: "UN".into(),
                        nome: "Unidade".into(),
                        fracionavel: false,
                    })
                    .map_err(ErroArmazenamento::Dominio)?;
                    let produto = mod_estoque::Produto::novo(
                        empresa,
                        grupo,
                        "Refrigerante Cola 2L",
                        "22021000",
                        unidade,
                    )
                    .map_err(|e| ErroArmazenamento::Dominio(Erro::de_dominio(&e)))?;
                    repo.inserir_produto(&produto)
                        .map_err(ErroArmazenamento::Dominio)?;
                    repo.inserir_local(empresa, local, "Depósito", "Deposito")
                        .map_err(ErroArmazenamento::Dominio)?;
                    Ok((produto.id, local))
                },
            )
            .unwrap()
            .valor
    }

    /// Importa o XML e vincula o item ao produto; devolve a nota.
    fn importar_e_vincular(&self, xml: String, produto: Id) -> RelatorioImportacao {
        let r: RelatorioImportacao = self
            .executar(
                "compras.importar_nota_de_arquivo_xml.v1",
                &ImportarNotaDeArquivoXml { xml },
            )
            .unwrap();
        let itens: Vec<mod_compras::ItemNotaEntrada> = self.consultar(
            "compras.itens_da_nota.v1",
            &ItensDaNota {
                nota: r.nota_entrada,
            },
        );
        let (): () = self
            .executar(
                "compras.vincular_produto_manual.v1",
                &VincularProdutoManual {
                    item_nota: itens[0].id,
                    produto,
                },
            )
            .unwrap();
        r
    }

    fn confirmar(
        &self,
        nota: Id,
        local: Id,
        pago_no_ato: bool,
        parcelas: Option<Vec<ParcelaCompra>>,
    ) -> Result<EntradaConfirmada, Erro> {
        self.executar(
            "compras.confirmar_entrada.v1",
            &ConfirmarEntrada {
                nota_entrada: nota,
                local,
                gerar_titulo_a_pagar: Some(true),
                pago_no_ato: Some(pago_no_ato),
                parcelas,
            },
        )
    }

    fn estornar(&self, nota: Id, estornar_pagamento: bool) -> Result<EntradaFoiEstornada, Erro> {
        self.executar(
            "compras.estornar_entrada.v1",
            &EstornarEntrada {
                nota_entrada: nota,
                motivo: "Produto vinculado errado na conferência".into(),
                estornar_pagamento,
            },
        )
    }

    fn sql<T: rusqlite::types::FromSql + Send + 'static>(
        &self,
        sql: &'static str,
        p: Vec<Vec<u8>>,
    ) -> T {
        self.arm
            .leitor()
            .consultar(move |c| {
                c.query_row(sql, rusqlite::params_from_iter(p.iter()), |r| r.get(0))
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .unwrap()
    }

    fn saldo_no_local(&self, produto: Id, local: Id) -> Quantidade {
        Quantidade::interna(self.sql(
            "SELECT COALESCE(SUM(quantidade_disponivel), 0) FROM estoque_saldo_local
             WHERE produto = ?1 AND local = ?2",
            vec![produto.em_bytes().to_vec(), local.em_bytes().to_vec()],
        ))
    }

    /// As parcelas do título vivo da nota: (vencimento, valor, estado).
    fn parcelas_do_titulo(&self, nota: Id) -> Vec<(Data, Dinheiro, String)> {
        let nota = nota.em_bytes().to_vec();
        self.arm
            .leitor()
            .consultar(move |c| {
                let mut st = c
                    .prepare(
                        "SELECT p.vencimento, p.valor, p.estado FROM financeiro_parcela p
                         JOIN financeiro_titulo t ON t.id = p.titulo
                         WHERE t.origem_modulo = 'compras' AND t.origem_id = ?1
                           AND t.cancelado_em IS NULL
                         ORDER BY p.numero",
                    )
                    .unwrap();
                let l = st
                    .query_map([&nota], |r| {
                        Ok((
                            Data::de_dias(r.get::<_, i32>(0)?),
                            Dinheiro::centavos(r.get(1)?),
                            r.get::<_, String>(2)?,
                        ))
                    })
                    .unwrap()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();
                Ok(l)
            })
            .unwrap()
    }

    /// As contas do Razão com saldo diferente de zero.
    fn contas_com_saldo(&self) -> i64 {
        self.sql(
            "SELECT COUNT(*) FROM (
               SELECT p.conta, SUM(p.valor) AS s FROM razao_partida p
                 JOIN razao_lancamento l ON l.id = p.lancamento
                WHERE l.empresa = ?1 AND l.estado <> 'Previsto'
                GROUP BY p.conta HAVING s <> 0)",
            vec![self.empresa.em_bytes().to_vec()],
        )
    }

    fn devolver(&self, nota: Id, item: Id, quantidade: i64) -> Result<DevolucaoFeita, Erro> {
        self.executar(
            "compras.devolver_ao_fornecedor.v1",
            &DevolverAoFornecedor {
                nota_entrada: nota,
                itens: vec![ItemADevolver {
                    item,
                    quantidade: Quantidade::unidades(quantidade),
                }],
                motivo: "Produto com a embalagem avariada".into(),
            },
        )
    }

    /// Paga o que falta da parcela `numero` do título da nota, no vencimento (Pix).
    fn pagar_parcela(&self, nota: Id, numero: i64) {
        let ctx = self.ctx();
        self.arm
            .escritor()
            .executar(
                ContextoEscrita::novo(self.empresa, Id::novo(), Id::novo(), Id::novo()),
                move |uow| {
                    let (id, valor, baixado, vencimento): (Vec<u8>, i64, i64, i32) = uow
                        .conexao()
                        .query_row(
                            "SELECT p.id, p.valor, p.valor_baixado, p.vencimento
                             FROM financeiro_parcela p
                             JOIN financeiro_titulo t ON t.id = p.titulo
                             WHERE t.origem_modulo = 'compras' AND t.origem_id = ?1
                               AND p.numero = ?2",
                            rusqlite::params![nota.em_bytes().as_slice(), numero],
                            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                        )
                        .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                    mod_financeiro::baixar_pagamento_comum(
                        Id::de_bytes(id.try_into().unwrap()),
                        Dinheiro::centavos(valor - baixado),
                        Data::de_dias(vencimento),
                        mod_financeiro::MeioPagamento::Pix,
                        None,
                        &ctx,
                        uow,
                    )
                    .map_err(ErroArmazenamento::Dominio)
                },
            )
            .unwrap();
    }

    /// As parcelas do título vivo da nota: (valor, já baixado, estado).
    fn baixas(&self, nota: Id) -> Vec<(Dinheiro, Dinheiro, String)> {
        let nota = nota.em_bytes().to_vec();
        self.arm
            .leitor()
            .consultar(move |c| {
                let mut st = c
                    .prepare(
                        "SELECT p.valor, p.valor_baixado, p.estado FROM financeiro_parcela p
                         JOIN financeiro_titulo t ON t.id = p.titulo
                         WHERE t.origem_modulo = 'compras' AND t.origem_id = ?1
                           AND t.cancelado_em IS NULL
                         ORDER BY p.numero",
                    )
                    .unwrap();
                let l = st
                    .query_map([&nota], |r| {
                        Ok((
                            Dinheiro::centavos(r.get(0)?),
                            Dinheiro::centavos(r.get(1)?),
                            r.get::<_, String>(2)?,
                        ))
                    })
                    .unwrap()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();
                Ok(l)
            })
            .unwrap()
    }

    fn estado_da_nota(&self, nota: Id) -> String {
        self.sql(
            "SELECT estado FROM compras_nota_entrada WHERE id = ?1",
            vec![nota.em_bytes().to_vec()],
        )
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

/// Uma NF-e de R$ 50,00 (10 × R$ 4,00 + R$ 10,00 de frete), emitida em 01/09/2026, com as
/// duplicatas informadas.
fn xml(chave: char, duplicatas: &[(&str, &str)]) -> String {
    let cobr = if duplicatas.is_empty() {
        String::new()
    } else {
        let dups: String = duplicatas
            .iter()
            .enumerate()
            .map(|(i, (venc, valor))| {
                format!(
                    "<dup><nDup>{:03}</nDup><dVenc>{venc}</dVenc><vDup>{valor}</vDup></dup>",
                    i + 1
                )
            })
            .collect();
        format!("<cobr><fat><nFat>4598</nFat></fat>{dups}</cobr>")
    };
    let chave: String = std::iter::repeat_n(chave, 44).collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<nfeProc xmlns="http://www.portalfiscal.inf.br/nfe"><NFe>
<infNFe Id="NFe{chave}" versao="4.00">
  <ide><nNF>4598</nNF><serie>1</serie><dhEmi>2026-09-01T09:00:00-03:00</dhEmi></ide>
  <emit><CNPJ>{CNPJ_FORNECEDOR}</CNPJ><xNome>Distribuidora Alfa LTDA</xNome></emit>
  <det nItem="1"><prod><cProd>REF-1</cProd><xProd>REFRIG COLA PET 2L</xProd><NCM>22021000</NCM>
    <qCom>10.0000</qCom><vUnCom>4.000000</vUnCom><vProd>40.00</vProd></prod></det>
  <total><ICMSTot><vProd>40.00</vProd><vFrete>10.00</vFrete><vSeg>0.00</vSeg><vOutro>0.00</vOutro>
    <vNF>50.00</vNF></ICMSTot></total>
  {cobr}
</infNFe></NFe></nfeProc>"#
    )
}

/// A nota da devolução: 10 × R$ 4,00 com ICMS próprio (base 50,00 a 12%), ST (base 70,00 a
/// 17%: R$ 5,90), IPI de R$ 2,00 e R$ 10,00 de frete — R$ 57,90 em duas duplicatas.
fn xml_com_impostos() -> String {
    let chave: String = std::iter::repeat_n('7', 44).collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<nfeProc xmlns="http://www.portalfiscal.inf.br/nfe"><NFe>
<infNFe Id="NFe{chave}" versao="4.00">
  <ide><nNF>4600</nNF><serie>1</serie><dhEmi>2026-09-01T09:00:00-03:00</dhEmi></ide>
  <emit><CNPJ>{CNPJ_FORNECEDOR}</CNPJ><xNome>Distribuidora Alfa LTDA</xNome></emit>
  <det nItem="1"><prod><cProd>REF-1</cProd><xProd>REFRIG COLA PET 2L</xProd><NCM>22021000</NCM>
    <qCom>10.0000</qCom><vUnCom>4.000000</vUnCom><vProd>40.00</vProd><vFrete>10.00</vFrete></prod>
    <imposto><ICMS><ICMS10><orig>0</orig><CST>10</CST><modBC>3</modBC><vBC>50.00</vBC>
      <pICMS>12.00</pICMS><vICMS>6.00</vICMS><modBCST>4</modBCST><pMVAST>40.00</pMVAST>
      <vBCST>70.00</vBCST><pICMSST>17.00</pICMSST><vICMSST>5.90</vICMSST></ICMS10></ICMS>
      <IPI><IPITrib><vIPI>2.00</vIPI></IPITrib></IPI></imposto></det>
  <total><ICMSTot><vProd>40.00</vProd><vST>5.90</vST><vIPI>2.00</vIPI><vFrete>10.00</vFrete>
    <vSeg>0.00</vSeg><vOutro>0.00</vOutro><vNF>57.90</vNF></ICMSTot></total>
  <cobr><fat><nFat>4600</nFat></fat>
    <dup><nDup>001</nDup><dVenc>2026-10-01</dVenc><vDup>28.95</vDup></dup>
    <dup><nDup>002</nDup><dVenc>2026-11-01</dVenc><vDup>28.95</vDup></dup></cobr>
</infNFe></NFe></nfeProc>"#
    )
}

fn dia(m: u32, d: u32) -> Data {
    Data::de_ymd(2026, m, d).unwrap()
}

#[test]
fn as_duplicatas_do_xml_viram_as_parcelas_da_conta_a_pagar() {
    let b = Base::nova();
    let (produto, local) = b.produto_e_local();
    let r = b.importar_e_vincular(
        xml('1', &[("2026-10-01", "20.00"), ("2026-11-01", "30.00")]),
        produto,
    );
    let parcelas: Vec<ParcelaCompra> = b.consultar(
        "compras.parcelas_da_nota.v1",
        &ParcelasDaNota {
            nota: r.nota_entrada,
        },
    );
    assert_eq!(parcelas.len(), 2);
    assert_eq!(parcelas[0].numero, "001");

    let ok = b.confirmar(r.nota_entrada, local, false, None).unwrap();
    assert!(ok.titulo.is_some() && !ok.pago);
    assert_eq!(
        b.parcelas_do_titulo(r.nota_entrada),
        vec![
            (dia(10, 1), Dinheiro::reais(20), "Aberta".to_owned()),
            (dia(11, 1), Dinheiro::reais(30), "Aberta".to_owned()),
        ]
    );
}

#[test]
fn duplicatas_que_nao_fecham_deixam_aviso_e_a_confirmacao_pede_as_certas() {
    let b = Base::nova();
    let (produto, local) = b.produto_e_local();
    let r = b.importar_e_vincular(xml('2', &[("2026-10-01", "45.00")]), produto);
    let alerta: Option<String> = b.sql(
        "SELECT alerta FROM compras_nota_entrada WHERE id = ?1",
        vec![r.nota_entrada.em_bytes().to_vec()],
    );
    assert!(
        alerta.is_some_and(|a| a.contains("Confira as parcelas")),
        "a nota avisa que as duplicatas não fecham"
    );
    let erro = b.confirmar(r.nota_entrada, local, false, None).unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ENTRADA_INVALIDA);

    let p = |m, d, reais| ParcelaCompra {
        numero: String::new(),
        vencimento: dia(m, d),
        valor: Dinheiro::reais(reais),
    };
    let certas = vec![p(10, 1, 15), p(10, 31, 15), p(11, 30, 20)];
    b.confirmar(r.nota_entrada, local, false, Some(certas.clone()))
        .unwrap();
    assert_eq!(b.parcelas_do_titulo(r.nota_entrada).len(), 3);
    let usadas: Vec<ParcelaCompra> = b.consultar(
        "compras.parcelas_da_nota.v1",
        &ParcelasDaNota {
            nota: r.nota_entrada,
        },
    );
    assert_eq!(usadas, certas, "a nota guarda as parcelas usadas");
}

#[test]
fn estornar_tira_do_estoque_cancela_a_conta_e_a_nota_volta_para_a_conferencia() {
    let b = Base::nova();
    let (produto, local) = b.produto_e_local();
    let r = b.importar_e_vincular(xml('3', &[]), produto);
    b.confirmar(r.nota_entrada, local, false, None).unwrap();
    assert_eq!(b.saldo_no_local(produto, local), Quantidade::unidades(10));
    assert_eq!(
        b.parcelas_do_titulo(r.nota_entrada),
        vec![(dia(9, 1), Dinheiro::reais(50), "Aberta".to_owned())],
        "sem duplicatas, uma parcela na emissão"
    );
    assert!(b.contas_com_saldo() > 0);

    let previa: PreviaEstornoEntrada = b.consultar(
        "compras.previa_estorno_entrada.v1",
        &PreviaDoEstornoDaEntrada {
            nota: r.nota_entrada,
        },
    );
    assert_eq!(previa.itens.len(), 1);
    assert_eq!(previa.itens[0].entrou, Quantidade::unidades(10));
    assert_eq!(previa.titulo, Some(Dinheiro::reais(50)));
    assert_eq!(previa.pago, Dinheiro::ZERO);
    assert_eq!(previa.bloqueio(), None);

    let curto: Result<EntradaFoiEstornada, _> = b.executar(
        "compras.estornar_entrada.v1",
        &EstornarEntrada {
            nota_entrada: r.nota_entrada,
            motivo: "errado".into(),
            estornar_pagamento: false,
        },
    );
    assert_eq!(curto.unwrap_err().codigo, CodigoErro::ENTRADA_INVALIDA);

    let feito = b.estornar(r.nota_entrada, false).unwrap();
    assert_eq!(feito.itens, 1);
    assert!(feito.titulo.is_some());
    assert_eq!(feito.pagamento_estornado, Dinheiro::ZERO);
    assert!(b.saldo_no_local(produto, local).e_zero());
    assert_eq!(
        b.contas_com_saldo(),
        0,
        "estoque e fornecedor voltam a zero no Razão"
    );
    assert!(
        b.parcelas_do_titulo(r.nota_entrada).is_empty(),
        "o título foi cancelado"
    );
    assert_eq!(b.estado_da_nota(r.nota_entrada), "AConferir");
    assert_eq!(
        b.estornar(r.nota_entrada, false).unwrap_err().codigo,
        CodigoErro::ESTADO_INVALIDO,
        "não estorna duas vezes"
    );

    // Corrigida, a nota entra de novo — e se estorna de novo (só o que está vivo).
    b.confirmar(r.nota_entrada, local, false, None).unwrap();
    assert_eq!(b.saldo_no_local(produto, local), Quantidade::unidades(10));
    assert_eq!(b.parcelas_do_titulo(r.nota_entrada).len(), 1);
    b.estornar(r.nota_entrada, false).unwrap();
    assert!(b.saldo_no_local(produto, local).e_zero());
    assert_eq!(b.contas_com_saldo(), 0);
}

#[test]
fn estorno_exige_pedir_o_estorno_do_pagamento_e_recusa_mercadoria_ja_vendida() {
    let b = Base::nova();
    let (produto, local) = b.produto_e_local();
    let r = b.importar_e_vincular(xml('4', &[("2026-10-01", "50.00")]), produto);
    let ok = b.confirmar(r.nota_entrada, local, true, None).unwrap();
    assert!(ok.pago);
    assert_eq!(
        b.parcelas_do_titulo(r.nota_entrada),
        vec![(dia(9, 1), Dinheiro::reais(50), "Quitada".to_owned())],
        "pago no ato: uma parcela, quitada — as duplicatas ficam de lado"
    );

    let previa: PreviaEstornoEntrada = b.consultar(
        "compras.previa_estorno_entrada.v1",
        &PreviaDoEstornoDaEntrada {
            nota: r.nota_entrada,
        },
    );
    assert_eq!(previa.pago, Dinheiro::reais(50));
    assert_eq!(
        b.estornar(r.nota_entrada, false).unwrap_err().codigo,
        CodigoErro::REGRA_VIOLADA,
        "com pagamento, só pedindo para estornar também"
    );

    // Vendeu 3: a entrada não se desfaz mais — e nada muda.
    let ctx = b.ctx();
    b.arm
        .escritor()
        .executar(
            ContextoEscrita::novo(b.empresa, Id::novo(), Id::novo(), Id::novo()),
            move |uow| {
                mod_estoque::registrar_saida_comum(
                    mod_estoque::DadosSaida {
                        produto,
                        local,
                        quantidade: Quantidade::unidades(3),
                        origem_modulo: "pdv",
                        origem_id: None,
                    },
                    &ctx,
                    uow,
                )
                .map_err(ErroArmazenamento::Dominio)
            },
        )
        .unwrap();
    let previa: PreviaEstornoEntrada = b.consultar(
        "compras.previa_estorno_entrada.v1",
        &PreviaDoEstornoDaEntrada {
            nota: r.nota_entrada,
        },
    );
    assert!(previa.bloqueio().is_some_and(|m| m.contains("só restam 7")));
    assert_eq!(
        b.estornar(r.nota_entrada, true).unwrap_err().codigo,
        CodigoErro::ESTOQUE_INSUFICIENTE
    );
    assert_eq!(b.saldo_no_local(produto, local), Quantidade::unidades(7));
    assert_eq!(b.estado_da_nota(r.nota_entrada), "Confirmada");
    assert_eq!(b.parcelas_do_titulo(r.nota_entrada)[0].2, "Quitada");
}

#[test]
fn estorno_com_o_pagamento_devolve_o_dinheiro_e_zera_o_razao() {
    let b = Base::nova();
    let (produto, local) = b.produto_e_local();
    let r = b.importar_e_vincular(xml('5', &[]), produto);
    b.confirmar(r.nota_entrada, local, true, None).unwrap();
    let feito = b.estornar(r.nota_entrada, true).unwrap();
    assert_eq!(feito.pagamento_estornado, Dinheiro::reais(50));
    assert!(b.saldo_no_local(produto, local).e_zero());
    assert_eq!(
        b.contas_com_saldo(),
        0,
        "estoque, fornecedor e a conta de onde saiu o dinheiro voltam a zero"
    );
    assert_eq!(b.estado_da_nota(r.nota_entrada), "AConferir");
    let _ = EstadoNotaEntrada::AConferir;
}

#[test]
fn a_nota_guarda_o_xml_original_e_vai_ao_contador_no_mes_da_emissao() {
    let b = Base::nova();
    let original = xml('7', &[]);
    let r: RelatorioImportacao = b
        .executar(
            "compras.importar_nota_de_arquivo_xml.v1",
            &ImportarNotaDeArquivoXml {
                xml: original.clone(),
            },
        )
        .unwrap();
    let do_mes = |mes| -> Vec<NotaDeEntradaDoMes> {
        b.consultar("compras.notas_do_mes.v1", &NotasDeEntradaDoMes { mes })
    };
    let setembro = do_mes(Competencia::nova(2026, 9));
    assert_eq!(setembro.len(), 1);
    let n = &setembro[0];
    assert_eq!(n.nota, r.nota_entrada);
    assert_eq!(n.xml.as_deref(), Some(original.as_str()), "o XML como veio");
    assert_eq!(n.chave.as_deref(), Some("7".repeat(44).as_str()));
    assert_eq!(n.fornecedor, "Distribuidora Alfa LTDA");
    assert_eq!((n.numero.as_str(), n.data_emissao), ("4598", dia(9, 1)));
    assert_eq!(n.valor_total, Dinheiro::reais(50));
    assert_eq!(n.estado, EstadoNotaEntrada::AConferir);
    assert!(do_mes(Competencia::nova(2026, 10)).is_empty());

    // Nota importada antes de o XML ser guardado: importar o mesmo arquivo de novo o guarda,
    // sem duplicar a nota.
    let nota = r.nota_entrada;
    b.arm
        .escritor()
        .executar(
            ContextoEscrita::novo(b.empresa, Id::novo(), Id::novo(), Id::novo()),
            move |uow| {
                uow.conexao()
                    .execute(
                        "UPDATE compras_nota_entrada SET xml = NULL WHERE id = ?1",
                        [nota.em_bytes().as_slice()],
                    )
                    .map(|_| ())
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            },
        )
        .unwrap();
    assert!(do_mes(Competencia::nova(2026, 9))[0].xml.is_none());
    let de_novo: RelatorioImportacao = b
        .executar(
            "compras.importar_nota_de_arquivo_xml.v1",
            &ImportarNotaDeArquivoXml {
                xml: original.clone(),
            },
        )
        .unwrap();
    assert_eq!(de_novo.nota_entrada, nota);
    let setembro = do_mes(Competencia::nova(2026, 9));
    assert_eq!(setembro.len(), 1);
    assert_eq!(setembro[0].xml.as_deref(), Some(original.as_str()));
}

#[test]
fn devolver_ao_fornecedor_abate_a_conta_e_o_que_sobra_vira_credito() {
    let b = Base::nova();
    let (produto, local) = b.produto_e_local();
    let r = b.importar_e_vincular(xml_com_impostos(), produto);
    b.confirmar(r.nota_entrada, local, false, None).unwrap();
    let itens: Vec<mod_compras::ItemNotaEntrada> = b.consultar(
        "compras.itens_da_nota.v1",
        &ItensDaNota {
            nota: r.nota_entrada,
        },
    );
    let item = itens[0].id;
    assert_eq!(itens[0].custo_total(), Dinheiro::centavos(5_790));

    // Mais do que entrou: recusado.
    assert_eq!(
        b.devolver(r.nota_entrada, item, 11).unwrap_err().codigo,
        CodigoErro::ENTRADA_INVALIDA
    );

    // 4 de 10 voltam: R$ 23,16 (40% de 57,90) abatem a primeira parcela.
    let primeira = b.devolver(r.nota_entrada, item, 4).unwrap();
    assert_eq!(primeira.valor_total, Dinheiro::centavos(2_316));
    assert_eq!(primeira.abatido, primeira.valor_total);
    assert_eq!((primeira.credito, primeira.nota_devolvida), (None, false));
    assert_eq!(b.saldo_no_local(produto, local), Quantidade::unidades(6));
    assert_eq!(
        b.baixas(r.nota_entrada),
        vec![
            (
                Dinheiro::centavos(2_895),
                Dinheiro::centavos(2_316),
                "Parcial".to_owned()
            ),
            (
                Dinheiro::centavos(2_895),
                Dinheiro::ZERO,
                "Aberta".to_owned()
            ),
        ]
    );
    assert_eq!(b.estado_da_nota(r.nota_entrada), "Confirmada");
    assert_eq!(
        b.estornar(r.nota_entrada, false).unwrap_err().codigo,
        CodigoErro::REGRA_VIOLADA,
        "com devolução, a entrada não se estorna"
    );

    // A NF-e de devolução fecha com o valor: produto 16,00 + ST 2,36 + IPI 0,80 + frete 4,00.
    let devolucoes: Vec<DevolucaoAoFornecedor> = b.consultar(
        "compras.devolucoes_da_nota.v1",
        &DevolucoesDaNota {
            nota_entrada: r.nota_entrada,
        },
    );
    assert_eq!(devolucoes.len(), 1);
    assert_eq!(devolucoes[0].itens[0].quantidade, Quantidade::unidades(4));
    let nota_id = r.nota_entrada;
    let (nota, todos) = b
        .arm
        .escritor()
        .executar(
            ContextoEscrita::novo(b.empresa, Id::novo(), Id::novo(), Id::novo()),
            move |uow| {
                let repo = mod_compras::RepositorioCompras::novo(uow);
                let nota = repo
                    .buscar_nota(nota_id)
                    .map_err(ErroArmazenamento::Dominio)?
                    .unwrap();
                let itens = repo
                    .itens_da_nota(nota_id)
                    .map_err(ErroArmazenamento::Dominio)?;
                Ok((nota, itens))
            },
        )
        .unwrap()
        .valor;
    let xml = cardeal_fiscal::interpretar(&xml_com_impostos()).unwrap();
    let dados = mod_compras::devolucao::dados_da_nfe(&nota, &todos, &devolucoes[0], Some(&xml));
    let i = &dados.itens[0];
    assert_eq!(i.valor_bruto, Dinheiro::reais(16));
    assert_eq!(
        i.icms,
        Some((
            Dinheiro::reais(20),
            cardeal_kernel::Percentual::de_str("12").unwrap()
        ))
    );
    assert!(i.com_st);
    assert_eq!(i.devolvido.valor_st, Dinheiro::centavos(236));
    assert_eq!(i.devolvido.base_st, Dinheiro::reais(28));
    assert_eq!(i.devolvido.ipi_devolvido, Dinheiro::centavos(80));
    assert_eq!(i.devolvido.frete, Dinheiro::reais(4));
    assert_eq!(
        i.valor_bruto - i.desconto + i.devolvido.soma(),
        primeira.valor_total,
        "a NF-e fecha com a devolução"
    );
    assert_eq!(dados.chave_da_compra, Some("7".repeat(44)));

    // A segunda parcela é paga; os 6 que restam voltam: R$ 34,74 — abatem os R$ 5,79 em
    // aberto e o resto (R$ 28,95) vira crédito com o fornecedor.
    b.pagar_parcela(r.nota_entrada, 2);
    let segunda = b.devolver(r.nota_entrada, item, 6).unwrap();
    assert_eq!(segunda.valor_total, Dinheiro::centavos(3_474));
    assert_eq!(segunda.abatido, Dinheiro::centavos(579));
    assert!(segunda.credito.is_some() && segunda.nota_devolvida);
    assert!(b.saldo_no_local(produto, local).e_zero());
    assert_eq!(b.estado_da_nota(r.nota_entrada), "Devolvida");
    let credito: i64 = b.sql(
        "SELECT p.valor FROM financeiro_parcela p JOIN financeiro_titulo t ON t.id = p.titulo
         WHERE t.especie = 'Receber' AND t.origem_modulo = 'compras_devolucao'",
        vec![],
    );
    assert_eq!(credito, 2_895);
    // Estoque e fornecedores zerados no Razão: sobram o banco (o que se pagou) e o crédito.
    assert_eq!(b.contas_com_saldo(), 2);
    assert_eq!(
        b.devolver(r.nota_entrada, item, 1).unwrap_err().codigo,
        CodigoErro::ESTADO_INVALIDO,
        "a nota devolvida não volta de novo"
    );
}
