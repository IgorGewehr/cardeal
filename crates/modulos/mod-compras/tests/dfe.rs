//! As notas de compra que chegam pela SEFAZ: a tarefa `compras.buscar_notas_na_sefaz` contra o
//! `FiscalSimulado` (que entrega como a SEFAZ: o resumo primeiro, a nota completa depois da
//! ciência), pelo despacho de verdade, com o certificado instalado e a empresa em produção.

#![allow(clippy::result_large_err)]

use std::sync::Arc;

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel, Sessao};
use cardeal_fiscal::{ChaveAcesso, FiscalSimulado, TipoDfe};
use cardeal_kernel::{CodigoErro, Erro, Id, Instante};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, Recursos, RegistroModulos};
use cardeal_storage::{
    Armazenamento, Cofre, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento,
};
use mod_compras::{
    EstadoNaSefaz, HoraDeBuscarNaSefaz, ImportarDeNovoDaSefaz, ModuloCompras, NotasNaSefaz,
    SituacaoDaBusca, TAREFA_BUSCAR_NA_SEFAZ,
};
use mod_empresa::{DadosFiscais, DefinirDadosFiscaisEmpresa, EnderecoFiscal, ModuloEmpresa};
use mod_fiscal::{AmbienteFiscal, DefinirAmbienteFiscal, InstalarCertificado, ProvedorFiscal};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tempfile::TempDir;

const CNPJ_EMPRESA: &str = "11222333000181";
const CNPJ_FORNECEDOR: &str = "45543915000181";

struct Mundo {
    _dir: TempDir,
    arm: Armazenamento,
    desp: Despachante,
    amb: Ambiente,
    sessao: Sessao,
    sim: Arc<FiscalSimulado>,
    empresa: Id,
}

impl Mundo {
    fn novo(ambiente: AmbienteFiscal) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let arm =
            Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("c.db"))).unwrap();
        arm.migrar(&[
            cardeal_ledger::migracoes::conjunto(),
            ModuloEmpresa.migracoes(),
            mod_clientes::ModuloClientes.migracoes(),
            mod_estoque::ModuloEstoque.migracoes(),
            mod_financeiro::ModuloFinanceiro.migracoes(),
            ModuloCompras.migracoes(),
            mod_fiscal::ModuloFiscal.migracoes(),
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
                           (id, razao_social, nome_fantasia, cnpj, regime, endereco, perfil, criado_em)
                         VALUES (?1,'Empresa Teste LTDA','Teste',?2,'SimplesNacional','{}','comercio',0)",
                            rusqlite::params![empresa.em_bytes().as_slice(), CNPJ_EMPRESA],
                        )
                        .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                    semear_plano_padrao(uow, empresa)
                        .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
                },
            )
            .unwrap();
        let mut rm = RegistroModulos::novo();
        for m in [
            &mod_empresa::MANIFESTO,
            &mod_clientes::MANIFESTO,
            &mod_estoque::MANIFESTO,
            &mod_financeiro::MANIFESTO,
            &mod_compras::MANIFESTO,
            &mod_fiscal::MANIFESTO,
        ] {
            rm.registrar(m).unwrap();
        }
        let efetivo = rm
            .resolver(
                &PedidoAtivacao::nova()
                    .com_modulo("empresa")
                    .com_modulo("compras")
                    .com_modulo("fiscal"),
            )
            .unwrap();
        let sim = Arc::new(FiscalSimulado::novo());
        let recursos = Recursos::novo()
            .com(Arc::new(Cofre::aleatorio()))
            .com(Arc::new(ProvedorFiscal::simulado(Arc::clone(&sim))));
        let amb = Ambiente::novo(empresa, efetivo).com_recursos(Arc::new(recursos));
        let mut papel = Papel::novo(empresa, "Dono");
        for p in [
            "empresa.dados.ver",
            "empresa.dados.editar",
            "fiscal.ver",
            "fiscal.configurar",
            "compras.entrada.ver",
            "compras.entrada.importar",
        ] {
            papel = papel.com_permissao(p);
        }
        let sessao = Sessao::abrir(EmissaoSessao::padrao(
            Id::novo(),
            Id::novo(),
            Escopo::empresa_inteira(empresa),
            AutorizacoesEfetivas::consolidar([&papel]),
            Instante::EPOCA,
        ));
        let desp = Despachante::construir(&[
            &ModuloEmpresa,
            &mod_clientes::ModuloClientes,
            &mod_estoque::ModuloEstoque,
            &mod_financeiro::ModuloFinanceiro,
            &ModuloCompras,
            &mod_fiscal::ModuloFiscal,
        ])
        .unwrap();
        let m = Self {
            _dir: dir,
            arm,
            desp,
            amb,
            sessao,
            sim,
            empresa,
        };
        let _: DadosFiscais = m
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
        let _: mod_fiscal::CertificadoResumo = m
            .executar(
                "fiscal.instalar_certificado.v1",
                &InstalarCertificado {
                    arquivo: pfx,
                    senha: "cardeal-teste".into(),
                },
            )
            .unwrap();
        let _: () = m
            .executar(
                "fiscal.definir_ambiente.v1",
                &DefinirAmbienteFiscal { ambiente },
            )
            .unwrap();
        m
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

    fn buscar(&self) -> Result<String, Erro> {
        self.desp.executar_tarefa(
            TAREFA_BUSCAR_NA_SEFAZ,
            &self.sessao,
            &self.amb,
            self.arm.leitor(),
            self.arm.escritor(),
        )
    }

    fn situacao(&self) -> SituacaoDaBusca {
        self.consultar("compras.notas_na_sefaz.v1", &NotasNaSefaz)
    }

    fn hora_de_buscar(&self) -> bool {
        self.consultar("compras.hora_de_buscar_na_sefaz.v1", &HoraDeBuscarNaSefaz)
    }

    /// O teste não espera a hora da SEFAZ (nem as esperas das notas).
    fn ja_pode(&self) {
        self.sql("UPDATE compras_dfe_estado SET proxima_consulta = 0");
        self.sql("UPDATE compras_dfe_nota SET proxima_tentativa = 0");
    }

    fn sql(&self, sql: &'static str) {
        self.arm
            .escritor()
            .executar(
                ContextoEscrita::novo(self.empresa, Id::novo(), Id::novo(), Id::novo()),
                move |uow| {
                    uow.conexao()
                        .execute(sql, [])
                        .map(|_| ())
                        .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
                },
            )
            .unwrap();
    }

    fn lancadas(&self) -> Vec<String> {
        let empresa = self.empresa;
        self.arm
            .leitor()
            .consultar(move |c| {
                let mut stmt = c
                    .prepare(
                        "SELECT chave_acesso FROM compras_nota_entrada WHERE empresa = ?1
                         ORDER BY chave_acesso",
                    )
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                let v = stmt
                    .query_map([empresa.em_bytes().as_slice()], |r| r.get(0))
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?
                    .collect::<rusqlite::Result<Vec<String>>>()
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()));
                v
            })
            .unwrap()
    }
}

fn chave(n: char) -> String {
    n.to_string().repeat(44)
}

/// Uma NF-e completa (`nfeProc`) do fornecedor para `destinatario`.
fn nota(chave: &str, destinatario: &str) -> String {
    format!(
        r#"<nfeProc xmlns="http://www.portalfiscal.inf.br/nfe"><NFe><infNFe Id="NFe{chave}" versao="4.00">
<ide><nNF>4598</nNF><serie>1</serie><tpNF>1</tpNF><finNFe>1</finNFe><dhEmi>2026-10-20T09:00:00-03:00</dhEmi></ide>
<emit><CNPJ>{CNPJ_FORNECEDOR}</CNPJ><xNome>Distribuidora Alfa LTDA</xNome></emit>
<dest><CNPJ>{destinatario}</CNPJ></dest>
<det nItem="1"><prod><cProd>PAR-M6</cProd><xProd>PARAFUSO M6</xProd><NCM>73181500</NCM><CFOP>5102</CFOP>
<qCom>10.0000</qCom><vUnCom>4.000000</vUnCom><vProd>40.00</vProd></prod></det>
<total><ICMSTot><vProd>40.00</vProd><vFrete>0.00</vFrete><vSeg>0.00</vSeg><vOutro>0.00</vOutro><vNF>40.00</vNF></ICMSTot></total>
</infNFe></NFe></nfeProc>"#
    )
}

/// O resumo (`resNFe`) dessa nota.
fn resumo(chave: &str, situacao: u8) -> String {
    format!(
        "<resNFe xmlns=\"http://www.portalfiscal.inf.br/nfe\" versao=\"1.01\"><chNFe>{chave}</chNFe>\
         <CNPJ>{CNPJ_FORNECEDOR}</CNPJ><xNome>Distribuidora Alfa LTDA</xNome><IE>1</IE>\
         <dhEmi>2026-10-20T09:00:00-03:00</dhEmi><tpNF>1</tpNF><vNF>40.00</vNF><digVal>x</digVal>\
         <dhRecbto>2026-10-20T09:00:05-03:00</dhRecbto><nProt>1</nProt><cSitNFe>{situacao}</cSitNFe></resNFe>"
    )
}

fn cancelamento(chave: &str) -> String {
    format!(
        "<resEvento versao=\"1.01\"><cOrgao>43</cOrgao><CNPJ>{CNPJ_FORNECEDOR}</CNPJ>\
         <chNFe>{chave}</chNFe><tpEvento>110111</tpEvento><nSeqEvento>1</nSeqEvento>\
         <xEvento>Cancelamento</xEvento></resEvento>"
    )
}

fn semear_nota(sim: &FiscalSimulado, nsu: u64, chave: &str, destinatario: &str) {
    sim.semear(
        nsu,
        &ChaveAcesso::nova(chave).unwrap(),
        nota(chave, destinatario),
    );
}

#[test]
fn a_busca_lanca_a_compra_da_ciencia_ao_resumo_e_registra_o_que_nao_e() {
    let m = Mundo::novo(AmbienteFiscal::Producao);
    let (compra, de_outro, so_resumo) = (chave('1'), chave('2'), chave('3'));
    semear_nota(&m.sim, 1, &compra, CNPJ_EMPRESA);
    semear_nota(&m.sim, 2, &de_outro, "12345678000195");
    m.sim.semear_resumo(
        3,
        &ChaveAcesso::nova(&so_resumo).unwrap(),
        resumo(&so_resumo, 1),
        nota(&so_resumo, CNPJ_EMPRESA),
    );
    assert!(m.hora_de_buscar());

    let r = m.buscar().unwrap();
    // A compra entra; a de outro destinatário fica registrada; o resumo recebe a ciência e a
    // nota completa vem pela consulta por chave — na mesma rodada.
    assert_eq!(
        r, "2 nota(s) lançada(s), 1 ignorada(s), 0 aguardando a SEFAZ",
        "{r}"
    );
    assert_eq!(m.lancadas(), vec![compra.clone(), so_resumo.clone()]);
    let s = m.situacao();
    assert_eq!(s.ultimo_nsu, 3);
    assert!(s.proxima_consulta.is_some(), "em dia: só daqui a uma hora");
    assert_eq!(s.ultimo_resultado.as_deref(), Some(r.as_str()));
    assert_eq!(s.notas.len(), 1);
    assert_eq!(s.notas[0].chave, de_outro);
    assert_eq!(s.notas[0].estado, EstadoNaSefaz::Ignorada);
    assert_eq!(
        s.notas[0].motivo.as_deref(),
        Some("o destinatário dela não é a sua empresa")
    );

    // Antes da hora: nem pergunta à SEFAZ.
    let consultas = m
        .sim
        .consultas_dfe
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(!m.hora_de_buscar());
    assert!(m
        .buscar()
        .unwrap()
        .starts_with("a SEFAZ só aceita nova consulta às "));
    assert_eq!(
        m.sim
            .consultas_dfe
            .load(std::sync::atomic::Ordering::SeqCst),
        consultas
    );

    // Depois da hora: a nota completa do resumo chega pelo NSU novo e não entra de novo.
    m.ja_pode();
    let r = m.buscar().unwrap();
    assert_eq!(
        r,
        "0 nota(s) lançada(s), 0 ignorada(s), 0 aguardando a SEFAZ"
    );
    assert_eq!(m.lancadas().len(), 2);
    assert_eq!(m.situacao().ultimo_nsu, 4);
}

#[test]
fn o_resumo_espera_ate_a_sefaz_liberar_e_o_cancelamento_o_tira_da_fila() {
    let m = Mundo::novo(AmbienteFiscal::Producao);
    let (espera, cancelada, ja_cancelada) = (chave('4'), chave('5'), chave('6'));
    // Um resumo cuja nota completa a SEFAZ ainda não tem (nem depois da ciência).
    m.sim.semear_documento(
        1,
        TipoDfe::ResumoDaNota,
        &ChaveAcesso::nova(&espera).unwrap(),
        resumo(&espera, 1),
    );
    m.sim.semear_documento(
        2,
        TipoDfe::ResumoDaNota,
        &ChaveAcesso::nova(&cancelada).unwrap(),
        resumo(&cancelada, 1),
    );
    // Um resumo que já chega cancelado.
    m.sim.semear_documento(
        3,
        TipoDfe::ResumoDaNota,
        &ChaveAcesso::nova(&ja_cancelada).unwrap(),
        resumo(&ja_cancelada, 3),
    );
    let r = m.buscar().unwrap();
    assert_eq!(
        r,
        "0 nota(s) lançada(s), 1 ignorada(s), 2 aguardando a SEFAZ"
    );
    let s = m.situacao();
    let espera_ = s.notas.iter().find(|n| n.chave == espera).unwrap();
    assert_eq!(espera_.estado, EstadoNaSefaz::Aguardando);
    assert!(espera_.ciencia_em.is_some(), "a ciência foi dada");
    assert_eq!(
        espera_.motivo.as_deref(),
        Some("com ciência; a SEFAZ ainda não liberou a nota completa")
    );
    let ja = s.notas.iter().find(|n| n.chave == ja_cancelada).unwrap();
    assert_eq!(
        (ja.estado, ja.motivo.as_deref()),
        (EstadoNaSefaz::Ignorada, Some("cancelada pelo fornecedor"))
    );

    // O fornecedor cancela uma das que esperavam: ela sai da fila.
    m.sim.semear_documento(
        4,
        TipoDfe::ResumoDeEvento,
        &ChaveAcesso::nova(&cancelada).unwrap(),
        cancelamento(&cancelada),
    );
    m.ja_pode();
    let r = m.buscar().unwrap();
    assert_eq!(
        r,
        "0 nota(s) lançada(s), 0 ignorada(s), 1 aguardando a SEFAZ"
    );
    let s = m.situacao();
    let c = s.notas.iter().find(|n| n.chave == cancelada).unwrap();
    assert_eq!(c.estado, EstadoNaSefaz::Ignorada);
    assert_eq!(c.motivo.as_deref(), Some("cancelada pelo fornecedor"));
}

#[test]
fn nota_lancada_que_o_fornecedor_cancela_vira_aviso() {
    let m = Mundo::novo(AmbienteFiscal::Producao);
    let compra = chave('7');
    semear_nota(&m.sim, 1, &compra, CNPJ_EMPRESA);
    m.buscar().unwrap();
    m.sim.semear_documento(
        2,
        TipoDfe::ResumoDeEvento,
        &ChaveAcesso::nova(&compra).unwrap(),
        cancelamento(&compra),
    );
    m.ja_pode();
    let r = m.buscar().unwrap();
    assert!(
        r.contains(&format!(
            "o fornecedor cancelou a nota {compra} que já foi lançada — confira a entrada"
        )),
        "{r}"
    );
}

#[test]
fn consumo_indevido_e_api_fora_do_ar_nao_andam_o_nsu() {
    let m = Mundo::novo(AmbienteFiscal::Producao);
    semear_nota(&m.sim, 1, &chave('8'), CNPJ_EMPRESA);
    m.sim.simular_consumo_indevido(true);
    let r = m.buscar().unwrap();
    assert!(r.contains("a SEFAZ pediu uma hora de espera"), "{r}");
    let s = m.situacao();
    assert_eq!(s.ultimo_nsu, 0);
    assert!(s.proxima_consulta.is_some());
    assert!(m.lancadas().is_empty());

    m.sim.simular_consumo_indevido(false);
    m.sim.simular_fora_do_ar(true);
    m.ja_pode();
    let erro = m.buscar().unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::FISCAL_INDISPONIVEL);
    let s = m.situacao();
    assert_eq!(s.ultimo_nsu, 0);
    assert!(s
        .ultimo_resultado
        .unwrap()
        .contains("a SEFAZ não respondeu"));

    m.sim.simular_fora_do_ar(false);
    m.ja_pode();
    assert!(m.buscar().unwrap().starts_with("1 nota(s) lançada(s)"));
    assert_eq!(m.situacao().ultimo_nsu, 1);
}

#[test]
fn nota_que_nao_importa_fica_guardada_e_a_fila_anda() {
    let m = Mundo::novo(AmbienteFiscal::Producao);
    let (ruim, boa) = (chave('9'), chave('1'));
    m.sim.semear(
        1,
        &ChaveAcesso::nova(&ruim).unwrap(),
        "<nfeProc><NFe>sem fim",
    );
    semear_nota(&m.sim, 2, &boa, CNPJ_EMPRESA);
    let r = m.buscar().unwrap();
    assert_eq!(
        r,
        "1 nota(s) lançada(s), 0 ignorada(s), 1 não importou(aram), 0 aguardando a SEFAZ"
    );
    let s = m.situacao();
    assert_eq!(s.ultimo_nsu, 2, "a nota ruim não trava a fila");
    assert_eq!(s.notas[0].estado, EstadoNaSefaz::Erro);
    assert!(s.notas[0]
        .motivo
        .as_deref()
        .unwrap()
        .starts_with("XML ilegível"));
    // Importar de novo o mesmo XML ruim continua recusado, com o motivo.
    let erro = m
        .executar::<mod_compras::RelatorioImportacao>(
            "compras.importar_de_novo_da_sefaz.v1",
            &ImportarDeNovoDaSefaz { chave: ruim },
        )
        .unwrap_err();
    assert_ne!(erro.codigo, CodigoErro::FALHA_INTERNA);
}

#[test]
fn em_homologacao_a_nota_fica_registrada_e_nao_entra_no_estoque() {
    let m = Mundo::novo(AmbienteFiscal::Homologacao);
    let compra = chave('2');
    semear_nota(&m.sim, 1, &compra, CNPJ_EMPRESA);
    m.buscar().unwrap();
    assert!(m.lancadas().is_empty());
    let s = m.situacao();
    assert_eq!(s.notas[0].estado, EstadoNaSefaz::Ignorada);
    assert!(s.notas[0]
        .motivo
        .as_deref()
        .unwrap()
        .starts_with("nota de homologação"));
}

#[test]
fn ciencia_fora_do_prazo_pede_o_xml_ao_fornecedor_e_sai_da_espera_ao_importar() {
    let m = Mundo::novo(AmbienteFiscal::Producao);
    let atrasada = chave('3');
    m.sim.semear_documento(
        1,
        TipoDfe::ResumoDaNota,
        &ChaveAcesso::nova(&atrasada).unwrap(),
        resumo(&atrasada, 1),
    );
    m.sim.programar(cardeal_fiscal::RetornoSefaz {
        situacao: cardeal_fiscal::SituacaoRetorno::Rejeitado,
        codigo: "596".into(),
        motivo: "Rejeição: Evento apresentado fora do prazo".into(),
        chave: None,
        protocolo: None,
        recebido_em: None,
        xml: None,
        xml_assinado: None,
        xml_cancelamento: None,
        erros: vec![],
    });
    m.buscar().unwrap();
    let s = m.situacao();
    assert_eq!(s.notas[0].estado, EstadoNaSefaz::Aguardando);
    assert!(s.notas[0]
        .motivo
        .as_deref()
        .unwrap()
        .contains("peça o XML ao fornecedor"));
    // Não insiste: a próxima rodada não pede a ciência de novo.
    m.ja_pode();
    m.buscar().unwrap();
    assert!(m.situacao().notas[0].ciencia_em.is_none());

    // O XML do fornecedor, importado pelo arquivo, tira a nota da espera.
    let _: mod_compras::RelatorioImportacao = m
        .executar(
            "compras.importar_nota_de_arquivo_xml.v1",
            &mod_compras::ImportarNotaDeArquivoXml {
                xml: nota(&atrasada, CNPJ_EMPRESA),
            },
        )
        .unwrap();
    assert!(m.situacao().notas.is_empty());
    m.ja_pode();
    assert_eq!(
        m.buscar().unwrap(),
        "0 nota(s) lançada(s), 0 ignorada(s), 0 aguardando a SEFAZ"
    );
}
