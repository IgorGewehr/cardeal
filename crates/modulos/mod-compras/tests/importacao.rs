//! O ciclo completo de importação de nota de entrada: `FiscalSimulado` no lugar da SEFAZ
//! real, resolução automática do fornecedor por CNPJ, cascata de casamento de produto,
//! rateio de frete, e os dois caminhos — confirmação totalmente automática (regra aprendida
//! e preferências) e revisão manual (`VincularProdutoManual` e `ConfirmarEntrada`) — cada
//! um terminando num movimento de estoque e (quando a preferência manda) um título a pagar
//! real no financeiro. Tudo contra SQLite de verdade, como vai rodar em produção.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Sessao};
use cardeal_fiscal::{ChaveAcesso, FiscalSimulado};
use cardeal_kernel::{Cnpj, Fuso, Id, Instante};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Ctx, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_compras::{
    confirmar_entrada_comum, importar_nota_da_sefaz, verificar_notas_na_sefaz, EstadoNotaEntrada,
    ItemNotaManual, LancarNotaManual, ModuloCompras, PreferenciasCompras, RateioPor,
    RelatorioImportacao, RepositorioCompras,
};
use mod_estoque::{ModuloEstoque, RepositorioEstoque};
use tempfile::TempDir;

const CNPJ_EMPRESA: &str = "11222333000181";
const CNPJ_FORNECEDOR: &str = "45543915000181";

fn base() -> (TempDir, Armazenamento, Id) {
    let dir = tempfile::tempdir().unwrap();
    let arm =
        Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("cardeal.db"))).unwrap();
    arm.migrar(&[
        cardeal_ledger::migracoes::conjunto(),
        mod_clientes::ModuloClientes.migracoes(),
        ModuloEstoque.migracoes(),
        mod_financeiro::ModuloFinanceiro.migracoes(),
        ModuloCompras.migracoes(),
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
                     VALUES (?1,'Teste LTDA','Teste',?2,'SimplesNacional','{}','comercio',0)",
                    rusqlite::params![empresa.em_bytes().as_slice(), CNPJ_EMPRESA],
                )
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
            semear_plano_padrao(uow, empresa).map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap();

    (dir, arm, empresa)
}

/// Um `Ctx` de verdade — empresa, conjunto efetivo e autorizações reais — montado sem
/// passar pelo lookup por nome do `Despachante`. É o que `verificar_notas_na_sefaz`,
/// `importar_nota_da_sefaz` e `confirmar_entrada_comum` esperam: elas não são `Comando`
/// (dependem de `PortaFiscal`, que o despacho ainda não injeta — ver `mod_compras::lib`),
/// mas continuam exigindo a mesma `UnidadeDeTrabalho` transacional de sempre.
fn ctx(empresa: Id) -> Ctx {
    let mut rm = RegistroModulos::novo();
    rm.registrar(&mod_clientes::MANIFESTO).unwrap();
    rm.registrar(&mod_estoque::MANIFESTO).unwrap();
    rm.registrar(&mod_financeiro::MANIFESTO).unwrap();
    rm.registrar(&mod_compras::MANIFESTO).unwrap();
    let efetivo = rm
        .resolver(&PedidoAtivacao::nova().com_modulo("compras"))
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

fn xml_nota(chave: &str, codigo_fornecedor: &str, descricao: &str, ncm: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<nfeProc xmlns="http://www.portalfiscal.inf.br/nfe">
  <NFe>
    <infNFe Id="NFe{chave}" versao="4.00">
      <ide>
        <nNF>4598</nNF>
        <serie>1</serie>
        <dhEmi>2026-09-01T09:00:00-03:00</dhEmi>
      </ide>
      <emit>
        <CNPJ>{CNPJ_FORNECEDOR}</CNPJ>
        <xNome>Distribuidora Alfa LTDA</xNome>
      </emit>
      <det nItem="1">
        <prod>
          <cProd>{codigo_fornecedor}</cProd>
          <xProd>{descricao}</xProd>
          <NCM>{ncm}</NCM>
          <qCom>10.0000</qCom>
          <vUnCom>4.000000</vUnCom>
          <vProd>40.00</vProd>
        </prod>
      </det>
      <total>
        <ICMSTot>
          <vProd>40.00</vProd>
          <vFrete>10.00</vFrete>
          <vSeg>0.00</vSeg>
          <vOutro>0.00</vOutro>
          <vNF>50.00</vNF>
        </ICMSTot>
      </total>
    </infNFe>
  </NFe>
</nfeProc>"#
    )
}

fn cadastrar_produto(arm: &Armazenamento, empresa: Id, nome: &str, ncm: &str) -> Id {
    let ctx_escrita = ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx_escrita, {
            let nome = nome.to_string();
            let ncm = ncm.to_string();
            move |uow| {
                let grupo = Id::novo();
                let unidade = Id::novo();
                let mut repo = RepositorioEstoque::novo(uow);
                repo.inserir_grupo_produto(empresa, grupo, "GERAL", "Geral", None)
                    .map_err(cardeal_storage::ErroArmazenamento::Dominio)?;
                let u = mod_estoque::Unidade {
                    id: unidade,
                    empresa,
                    sigla: "UN".to_string(),
                    nome: "Unidade".to_string(),
                    fracionavel: false,
                };
                repo.inserir_unidade(&u)
                    .map_err(cardeal_storage::ErroArmazenamento::Dominio)?;
                let produto = mod_estoque::Produto::novo(empresa, grupo, nome, &ncm, unidade)
                    .map_err(|e| {
                        cardeal_storage::ErroArmazenamento::Dominio(
                            cardeal_kernel::Erro::de_dominio(&e),
                        )
                    })?;
                repo.inserir_produto(&produto)
                    .map_err(cardeal_storage::ErroArmazenamento::Dominio)?;
                Ok(produto.id)
            }
        })
        .unwrap()
        .valor
}

fn cadastrar_local(arm: &Armazenamento, empresa: Id) -> Id {
    let ctx_escrita = ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx_escrita, move |uow| {
            let id = Id::novo();
            RepositorioEstoque::novo(uow)
                .inserir_local(empresa, id, "Depósito", "Deposito")
                .map_err(cardeal_storage::ErroArmazenamento::Dominio)?;
            Ok(id)
        })
        .unwrap()
        .valor
}

fn conta_movimentos_estoque(arm: &Armazenamento, empresa: Id) -> i64 {
    arm.leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM estoque_movimento WHERE empresa = ?1",
                [empresa.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap()
}

fn conta_titulos_a_pagar(arm: &Armazenamento, empresa: Id) -> i64 {
    arm.leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM financeiro_titulo WHERE empresa = ?1 AND origem_modulo = 'compras'",
                [empresa.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap()
}

#[test]
fn varredura_da_sefaz_confirma_sozinha_quando_tudo_ja_foi_aprendido() {
    let (_dir, arm, empresa) = base();
    let produto = cadastrar_produto(&arm, empresa, "Parafuso Sextavado M6", "73181500");
    let local = cadastrar_local(&arm, empresa);
    let chave = "1".repeat(44);

    let sim = std::sync::Arc::new(FiscalSimulado::novo());
    let cnpj_empresa = Cnpj::novo(CNPJ_EMPRESA).unwrap();
    let cnpj_fornecedor = Cnpj::novo(CNPJ_FORNECEDOR).unwrap();
    let chave_acesso = ChaveAcesso::nova(&chave).unwrap();
    sim.semear(
        cnpj_empresa,
        1,
        &chave_acesso,
        xml_nota(&chave, "PAR-M6", "PARAF SEXT M6 ZINC", "73181500"),
    );

    let sim_1 = std::sync::Arc::clone(&sim);
    let sim_2 = std::sync::Arc::clone(&sim);
    let ctx_escrita = ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx_escrita, move |uow| {
            // Preferências: confirmação automática ligada, com local padrão.
            let prefs = PreferenciasCompras {
                empresa,
                confirma_automaticamente_quando_tudo_casa: true,
                gera_titulo_a_pagar: true,
                rateio_por: RateioPor::Valor,
                local_padrao: Some(local),
            };
            RepositorioCompras::novo(uow)
                .definir_preferencias(&prefs)
                .map_err(cardeal_storage::ErroArmazenamento::Dominio)?;

            // O fornecedor só existe depois da primeira importação; para este teste,
            // cadastra-se antes com o mesmo CNPJ que o XML vai casar.
            let agora = uow.agora();
            let pessoa = mod_clientes::ConstrutorPessoa::nova(
                empresa,
                mod_clientes::TipoPessoa::Juridica,
                "Distribuidora Alfa LTDA",
                mod_clientes::Papel::Fornecedor,
                agora,
                agora.data(Fuso::BRASILIA),
            )
            .construir()
            .unwrap();
            let doc = mod_clientes::DocumentoPessoa::novo(
                empresa,
                pessoa.id,
                mod_clientes::TipoDocumento::Cnpj,
                &cnpj_fornecedor.sem_mascara(),
            )
            .unwrap();
            {
                let mut repo = mod_clientes::RepositorioClientes::novo(uow);
                repo.inserir_pessoa(&pessoa)
                    .map_err(cardeal_storage::ErroArmazenamento::Dominio)?;
                repo.inserir_documento(&doc)
                    .map_err(cardeal_storage::ErroArmazenamento::Dominio)?;
            }

            // Já existe uma regra aprendida (de uma importação manual anterior, simulada)
            // para este fornecedor + código — é o que permite a confirmação automática.
            RepositorioCompras::novo(uow)
                .upsert_regra_casamento(&mod_compras::RegraCasamentoAprendida {
                    empresa,
                    fornecedor: pessoa.id,
                    codigo_fornecedor: "PAR-M6".to_string(),
                    produto,
                    aprendido_em: agora,
                })
                .map_err(cardeal_storage::ErroArmazenamento::Dominio)?;

            let relatorios = verificar_notas_na_sefaz(sim_1.as_ref(), &ctx(empresa), uow)
                .map_err(cardeal_storage::ErroArmazenamento::Dominio)?;
            assert_eq!(relatorios.len(), 1);
            assert_eq!(relatorios[0].estado, EstadoNotaEntrada::Confirmada);
            assert_eq!(relatorios[0].itens_nao_casados, 0);
            Ok(())
        })
        .unwrap();

    assert_eq!(conta_movimentos_estoque(&arm, empresa), 1);
    assert_eq!(conta_titulos_a_pagar(&arm, empresa), 1);

    // Varrer de novo não duplica nada — a mesma chave já foi importada.
    let ctx_escrita2 = ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx_escrita2, move |uow| {
            let relatorios = verificar_notas_na_sefaz(sim_2.as_ref(), &ctx(empresa), uow)
                .map_err(cardeal_storage::ErroArmazenamento::Dominio)?;
            assert!(relatorios.is_empty());
            Ok(())
        })
        .unwrap();
    assert_eq!(conta_movimentos_estoque(&arm, empresa), 1);
}

#[test]
fn nota_sem_casamento_fica_a_conferir_ate_vinculo_manual() {
    let (_dir, arm, empresa) = base();
    let produto = cadastrar_produto(&arm, empresa, "Bateria Original 6 Células", "85076000");
    let local = cadastrar_local(&arm, empresa);
    let chave = "2".repeat(44);

    let ctx_escrita = ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo());
    let (nota_id, item_id) = arm
        .escritor()
        .executar(ctx_escrita, {
            let xml = xml_nota(
                &chave,
                "BAT-XPTO",
                "PECA SEM CORRESPONDENCIA NENHUMA",
                "99999999",
            );
            move |uow| {
                let nota_xml = cardeal_fiscal::interpretar(&xml)
                    .map_err(|e| cardeal_kernel::Erro::de_dominio(&e))
                    .map_err(cardeal_storage::ErroArmazenamento::Dominio)?;
                let relatorio = importar_nota_da_sefaz(&nota_xml, &ctx(empresa), uow)
                    .map_err(cardeal_storage::ErroArmazenamento::Dominio)?;
                assert_eq!(relatorio.estado, EstadoNotaEntrada::AConferir);
                assert_eq!(relatorio.itens_nao_casados, 1);

                let itens = RepositorioCompras::novo(uow)
                    .itens_da_nota(relatorio.nota_entrada)
                    .map_err(cardeal_storage::ErroArmazenamento::Dominio)?;
                Ok((relatorio.nota_entrada, itens[0].id))
            }
        })
        .unwrap()
        .valor;

    // Vínculo manual + confirmação, sem gerar título a pagar desta vez.
    let ctx_escrita2 = ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx_escrita2, move |uow| {
            let mut item = RepositorioCompras::novo(uow)
                .buscar_item(item_id)
                .map_err(cardeal_storage::ErroArmazenamento::Dominio)?
                .unwrap();
            item.vincular(produto);
            RepositorioCompras::novo(uow)
                .atualizar_item(&item)
                .map_err(cardeal_storage::ErroArmazenamento::Dominio)?;

            let titulo = confirmar_entrada_comum(nota_id, local, false, &ctx(empresa), uow)
                .map_err(cardeal_storage::ErroArmazenamento::Dominio)?;
            assert!(titulo.is_none());
            Ok(())
        })
        .unwrap();

    assert_eq!(conta_movimentos_estoque(&arm, empresa), 1);
    assert_eq!(conta_titulos_a_pagar(&arm, empresa), 0);
}

fn sessao_com(empresa: Id, permissoes: &[&str]) -> Sessao {
    let mut papel = cardeal_auth::Papel::novo(empresa, "Testador");
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

#[test]
fn lancar_nota_manual_sugere_por_ncm_e_fica_a_conferir_sem_regra_aprendida() {
    let (_dir, arm, empresa) = base();
    let produto = cadastrar_produto(&arm, empresa, "Parafuso Sextavado M6", "73181500");
    let d = Despachante::construir(&[
        &mod_clientes::ModuloClientes,
        &ModuloEstoque,
        &mod_financeiro::ModuloFinanceiro,
        &ModuloCompras,
    ])
    .unwrap();
    let s = sessao_com(empresa, &["compras.entrada.importar"]);
    let mut rm = RegistroModulos::novo();
    rm.registrar(&mod_clientes::MANIFESTO).unwrap();
    rm.registrar(&mod_estoque::MANIFESTO).unwrap();
    rm.registrar(&mod_financeiro::MANIFESTO).unwrap();
    rm.registrar(&mod_compras::MANIFESTO).unwrap();
    let efetivo = rm
        .resolver(&PedidoAtivacao::nova().com_modulo("compras"))
        .unwrap();
    let amb = Ambiente::novo(empresa, efetivo);

    let cmd = LancarNotaManual {
        fornecedor_cnpj: CNPJ_FORNECEDOR.to_string(),
        fornecedor_nome: "Distribuidora Alfa LTDA".to_string(),
        numero: "900".to_string(),
        serie: "1".to_string(),
        data_emissao: cardeal_kernel::Data::hoje(Fuso::BRASILIA),
        itens: vec![ItemNotaManual {
            codigo_fornecedor: "PAR-M6".to_string(),
            descricao: "Parafuso Sextavado M6".to_string(),
            ncm: "73181500".to_string(),
            quantidade: cardeal_kernel::Quantidade::unidades(10),
            valor_unitario: cardeal_kernel::Preco::reais(4),
        }],
        valor_frete: cardeal_kernel::Dinheiro::reais(10),
        valor_seguro: cardeal_kernel::Dinheiro::ZERO,
        valor_outras_despesas: cardeal_kernel::Dinheiro::ZERO,
    };
    let saida = d
        .executar_comando(
            "compras.lancar_nota_manual.v1",
            &carga(&cmd),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap();
    let relatorio: RelatorioImportacao = postcard::from_bytes(&saida).unwrap();

    assert_eq!(relatorio.estado, EstadoNotaEntrada::AConferir);
    // Só uma regra aprendida daria `Casado` direto; sem ela, NCM + descrição idêntica rende
    // no máximo `SugestaoForte` — ainda "não casado" para fins de confirmação automática
    // (`docs/modulos/compras.md` §5, a cascata: regra aprendida → sugestão → nada).
    assert_eq!(relatorio.itens_nao_casados, 1);

    let (estado_casamento, produto_casado): (String, Vec<u8>) = arm
        .leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT estado_casamento, produto_casado FROM compras_item_nota_entrada WHERE nota_entrada = ?1",
                [relatorio.nota_entrada.em_bytes().as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| cardeal_storage::ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap();
    assert_eq!(estado_casamento, "SugestaoForte");
    assert_eq!(Id::de_bytes(produto_casado.try_into().unwrap()), produto);
}
