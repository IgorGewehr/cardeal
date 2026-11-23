//! A DPS e o cancelamento contra os **XSD oficiais** (vigente e NT 009, com o `xmllint`) e a
//! assinatura conferida por um verificador independente (`xmlsec1`). Sem as ferramentas na
//! máquina, esses dois testes avisam e passam — a CI do projeto as instala.

use std::path::{Path, PathBuf};
use std::process::Command;

use cardeal_kernel::{Cnpj, Cpf, Data, Dinheiro, Fuso, Hora, Instante, Percentual, Uf};

use super::*;
use crate::emissao::{Crt, DocumentoConsumidor, EnderecoEmitente};
use crate::nfse::{MotivoCancelamentoNfse, OpcaoSimples, TomadorNfse};

const SENHA: &str = "cardeal-teste";

fn dados() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/dados")
}

fn chave() -> ChaveDeAssinatura {
    let pfx = std::fs::read(dados().join("aes256.pfx")).unwrap();
    abrir_para_assinar(&pfx, SENHA).unwrap()
}

fn instante() -> Instante {
    Instante::de_data_hora(
        Data::de_ymd(2026, 11, 23).unwrap(),
        Hora::de_hms(10, 15, 0).unwrap(),
        Fuso::BRASILIA,
    )
}

fn endereco() -> EnderecoEmitente {
    EnderecoEmitente {
        logradouro: "Rua Sinimbu".into(),
        numero: "1500".into(),
        complemento: "Sala 2".into(),
        bairro: "Centro".into(),
        codigo_municipio: "4305108".into(),
        municipio: "Caxias do Sul".into(),
        uf: Uf::Rs,
        cep: "95020-002".into(),
        telefone: None,
    }
}

fn nota(crt: Crt) -> Nfse {
    Nfse {
        prestador: Emitente {
            cnpj: Cnpj::novo("11222333000181").unwrap(),
            razao_social: "EMPRESA TESTE LTDA".into(),
            nome_fantasia: "Assistência Teste".into(),
            inscricao_estadual: "ISENTO".into(),
            inscricao_municipal: Some("12345".into()),
            crt,
            endereco: endereco(),
        },
        tomador: TomadorNfse {
            documento: Some(DocumentoConsumidor::Cpf(Cpf::novo("52998224725").unwrap())),
            nome: "Maria da Silva & Filhos <ME>".into(),
            endereco: Some(endereco()),
            email: Some("maria@exemplo.com.br".into()),
        },
        codigo_tributacao_nacional: "140101".into(),
        item_lc116: "14.01".into(),
        codigo_tributacao_municipal: None,
        nbs: Some("1.0501.10.00".into()),
        discriminacao: "OS 42 – iPhone 13: troca de tela (R$ 450,00)\r\nGarantia de 90 dias".into(),
        valor_servicos: Dinheiro::centavos(45_000),
        desconto: Dinheiro::centavos(5_000),
        aliquota: Percentual::de_str("6").unwrap(),
        iss_retido: false,
        serie: 1,
        numero: 42,
        competencia: Data::de_ymd(2026, 11, 23).unwrap(),
        informacoes: Some("Atendimento no balcão".into()),
        opcao_simples: None,
    }
}

fn tem(programa: &str) -> bool {
    Command::new(programa).arg("--version").output().is_ok()
}

/// Os esquemas de uma pasta, prontos para o `xmllint`. O `TSSerieDPS` do vigente usa
/// `^0{0,4}\d{1,5}$` — no XSD `^` e `$` não são âncoras (o validador da Sefin, .NET, os trata como
/// âncoras; o libxml2, como letras, e recusaria qualquer série). Só esse padrão é trocado pelo
/// equivalente ancorado; o resto do esquema é o oficial.
fn esquemas(pasta: &str, destino: &Path) -> PathBuf {
    let origem = dados().join("nfse_xsd").join(pasta);
    let alvo = destino.join(pasta);
    std::fs::create_dir_all(&alvo).unwrap();
    for e in std::fs::read_dir(&origem).unwrap() {
        let e = e.unwrap();
        let texto = std::fs::read_to_string(e.path()).unwrap();
        let texto = texto.replace("value=\"^0{0,4}\\d{1,5}$\"", "value=\"0{0,4}\\d{1,5}\"");
        std::fs::write(alvo.join(e.file_name()), texto).unwrap();
    }
    alvo
}

/// Valida `xml` contra o XSD (`pasta/arquivo`); devolve a saída do `xmllint` se não passar.
fn validar(xml: &str, xsd: &str, nome: &str) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("cardeal-nfse-{}-{nome}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let arquivo = dir.join("doc.xml");
    std::fs::write(&arquivo, xml).unwrap();
    let (pasta, nome_xsd) = xsd.split_once('/').unwrap();
    let saida = Command::new("xmllint")
        .args(["--noout", "--nonet", "--schema"])
        .arg(esquemas(pasta, &dir).join(nome_xsd))
        .arg(&arquivo)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    if saida.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&saida.stderr).into_owned())
    }
}

/// Confere a assinatura com o `xmlsec1` (o certificado do `KeyInfo` como chave).
fn conferir_assinatura(xml: &str, elemento: &str, nome: &str) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("cardeal-dsig-{}-{nome}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (arquivo, certificado) = (dir.join("doc.xml"), dir.join("cert.der"));
    std::fs::write(&arquivo, xml).unwrap();
    std::fs::write(&certificado, &chave().certificado).unwrap();
    let saida = Command::new("xmlsec1")
        .env("OPENSSL_ENABLE_SHA1_SIGNATURES", "1")
        .args(["--verify", "--id-attr:Id", elemento, "--pubkey-cert-der"])
        .arg(&certificado)
        .arg(&arquivo)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    if saida.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&saida.stderr).into_owned())
    }
}

#[test]
fn o_id_da_dps_tem_45_posicoes() {
    let id = id_dps("4305108", "11222333000181", 1, 42);
    assert_eq!(id, "DPS430510821122233300018100001000000000000042");
    assert_eq!(id.len(), 45);
}

#[test]
fn a_dps_sai_na_ordem_do_xsd_e_com_o_texto_limpo() {
    let d = montar_dps(
        &nota(Crt::SimplesNacional),
        AmbienteFiscal::Homologacao,
        LeiauteDps::Nt009,
        instante(),
    )
    .unwrap();
    let x = &d.inf_dps;
    assert!(x.starts_with("<infDPS Id=\"DPS4305108"), "{x}");
    assert!(
        x.contains("<tpAmb>2</tpAmb><dhEmi>2026-11-23T10:15:00-03:00</dhEmi>"),
        "{x}"
    );
    assert!(x.contains("<tpEmit>1</tpEmit><finNFSe>0</finNFSe><cLocEmi>4305108</cLocEmi>"));
    assert!(x.contains("<regTrib><opSimpNac>3</opSimpNac><regApTribSN>1</regApTribSN><regEspTrib>0</regEspTrib></regTrib>"));
    assert!(x.contains("<xNome>Maria da Silva &amp; Filhos &lt;ME&gt;</xNome>"));
    assert!(x.contains(
        "<xDescServ>OS 42 - iPhone 13: troca de tela (R$ 450,00)\nGarantia de 90 dias</xDescServ>"
    ));
    assert!(x.contains("<cNBS>105011000</cNBS>"));
    assert!(x.contains(
        "<vServ>450.00</vServ></vServPrest><vDescCondIncond><vDescIncond>50.00</vDescIncond>"
    ));
    assert!(
        x.contains("<tribMun><tribISSQN>1</tribISSQN><tpRetISSQN>1</tpRetISSQN></tribMun>"),
        "Simples não manda pAliq"
    );
    assert!(x.contains("<totTrib><pTotTribSN>6.00</pTotTribSN></totTrib>"));
    let vigente = montar_dps(
        &nota(Crt::SimplesNacional),
        AmbienteFiscal::Producao,
        LeiauteDps::Vigente,
        instante(),
    )
    .unwrap();
    assert!(!vigente.inf_dps.contains("finNFSe") && vigente.inf_dps.contains("<tpAmb>1</tpAmb>"));
}

#[test]
fn regime_normal_manda_a_aliquota_e_o_mei_nao_manda_percentual_do_simples() {
    let mut n = nota(Crt::RegimeNormal);
    n.aliquota = Percentual::de_str("5").unwrap();
    let x = montar_dps(
        &n,
        AmbienteFiscal::Homologacao,
        LeiauteDps::Nt009,
        instante(),
    )
    .unwrap()
    .inf_dps;
    assert!(x.contains("<opSimpNac>1</opSimpNac><regEspTrib>0</regEspTrib>"));
    assert!(x.contains("<tpRetISSQN>1</tpRetISSQN><pAliq>5.00</pAliq>"));
    assert!(x.contains("<indTotTrib>0</indTotTrib>"));
    let mut mei = nota(Crt::SimplesNacional);
    mei.opcao_simples = Some(OpcaoSimples::Mei);
    let x = montar_dps(
        &mei,
        AmbienteFiscal::Homologacao,
        LeiauteDps::Nt009,
        instante(),
    )
    .unwrap()
    .inf_dps;
    assert!(
        x.contains("<opSimpNac>2</opSimpNac><regEspTrib>0</regEspTrib>"),
        "MEI não tem regApTribSN"
    );
    assert!(x.contains("<indTotTrib>0</indTotTrib>"));
}

#[test]
fn o_que_impede_vem_em_frases() {
    let mut n = nota(Crt::SimplesNacional);
    n.codigo_tributacao_nacional = "1401".into();
    n.nbs = Some("123".into());
    n.desconto = Dinheiro::centavos(45_000);
    let p = montar_dps(
        &n,
        AmbienteFiscal::Homologacao,
        LeiauteDps::Nt009,
        instante(),
    )
    .unwrap_err();
    assert_eq!(p.len(), 3, "{p:?}");
    let mut n = nota(Crt::RegimeNormal);
    n.aliquota = Percentual::de_str("12").unwrap();
    let p = montar_dps(
        &n,
        AmbienteFiscal::Homologacao,
        LeiauteDps::Nt009,
        instante(),
    )
    .unwrap_err();
    assert!(p[0].contains("alíquota"), "{p:?}");
}

#[test]
fn a_dps_assinada_valida_nos_dois_xsd_e_a_assinatura_confere() {
    if !tem("xmllint") || !tem("xmlsec1") {
        eprintln!("AVISO: sem xmllint/xmlsec1 — a validação contra o XSD oficial e a conferência da assinatura não rodaram");
        return;
    }
    let k = chave();
    let mut sem_tomador = nota(Crt::SimplesNacional);
    sem_tomador.tomador = TomadorNfse {
        documento: None,
        nome: String::new(),
        endereco: None,
        email: None,
    };
    sem_tomador.desconto = Dinheiro::ZERO;
    sem_tomador.nbs = None;
    sem_tomador.informacoes = None;
    let mut normal = nota(Crt::RegimeNormal);
    normal.aliquota = Percentual::de_str("2,5").unwrap();
    normal.tomador.documento = Some(DocumentoConsumidor::Cnpj(
        Cnpj::novo("11444777000161").unwrap(),
    ));
    normal.iss_retido = true;
    for (i, n) in [nota(Crt::SimplesNacional), sem_tomador, normal]
        .iter()
        .enumerate()
    {
        for (leiaute, pasta, outra) in [
            (LeiauteDps::Vigente, "vigente", "nt009"),
            (LeiauteDps::Nt009, "nt009", "vigente"),
        ] {
            let (_, xml) =
                dps_assinada(n, AmbienteFiscal::Homologacao, leiaute, instante(), &k).unwrap();
            let nome = format!("{i}-{pasta}");
            validar(&xml, &format!("{pasta}/DPS_v1.01.xsd"), &nome)
                .unwrap_or_else(|e| panic!("nota {i} no leiaute {pasta}: {e}\n{xml}"));
            assert!(
                validar(
                    &xml,
                    &format!("{outra}/DPS_v1.01.xsd"),
                    &format!("{nome}-x")
                )
                .is_err(),
                "o leiaute {pasta} não pode valer no {outra}"
            );
            conferir_assinatura(&xml, "infDPS", &nome)
                .unwrap_or_else(|e| panic!("assinatura da nota {i} ({pasta}): {e}"));
            // Mexer num valor depois de assinar quebra a assinatura.
            let adulterado = xml.replacen("<vServ>450.00</vServ>", "<vServ>45.00</vServ>", 1);
            if adulterado != xml {
                assert!(
                    conferir_assinatura(&adulterado, "infDPS", &format!("{nome}-adulterado"))
                        .is_err()
                );
            }
        }
    }
}

#[test]
fn o_cancelamento_assinado_valida_e_confere() {
    if !tem("xmllint") || !tem("xmlsec1") {
        eprintln!("AVISO: sem xmllint/xmlsec1 — o pedido de cancelamento não foi validado");
        return;
    }
    let c = CancelamentoNfse {
        chave: "43051082112223330001810000000000004226110123456789".into(),
        numero: 42,
        motivo: MotivoCancelamentoNfse::Duplicidade,
        justificativa: "nota".into(),
    };
    let xml = cancelamento_assinado(
        &c,
        "11222333000181",
        AmbienteFiscal::Homologacao,
        instante(),
        &chave(),
    )
    .unwrap();
    assert!(
        xml.contains("<cMotivo>9</cMotivo>"),
        "duplicidade é \"outros\" no padrão nacional"
    );
    assert!(
        xml.contains("<xMotivo>Nota em duplicidade - nota</xMotivo>"),
        "{xml}"
    );
    assert!(xml.contains("Id=\"PRE43051082112223330001810000000000004226110123456789101101\""));
    for pasta in ["vigente", "nt009"] {
        validar(
            &xml,
            &format!("{pasta}/pedRegEvento_v1.01.xsd"),
            &format!("cancel-{pasta}"),
        )
        .unwrap_or_else(|e| panic!("{pasta}: {e}\n{xml}"));
    }
    conferir_assinatura(&xml, "infPedReg", "cancel").unwrap();
}

#[test]
fn a_nota_guardada_no_formato_anterior_ainda_le() {
    #[derive(serde::Serialize)]
    struct Antiga<'a> {
        prestador: &'a Emitente,
        tomador: &'a TomadorNfse,
        codigo_tributacao_nacional: &'a str,
        item_lc116: &'a str,
        codigo_tributacao_municipal: Option<String>,
        nbs: Option<String>,
        discriminacao: &'a str,
        valor_servicos: Dinheiro,
        desconto: Dinheiro,
        aliquota: Percentual,
        iss_retido: bool,
        serie: u32,
        numero: u64,
        competencia: Data,
        informacoes: Option<String>,
    }
    let n = nota(Crt::SimplesNacional);
    let antiga = postcard::to_stdvec(&Antiga {
        prestador: &n.prestador,
        tomador: &n.tomador,
        codigo_tributacao_nacional: &n.codigo_tributacao_nacional,
        item_lc116: &n.item_lc116,
        codigo_tributacao_municipal: None,
        nbs: n.nbs.clone(),
        discriminacao: &n.discriminacao,
        valor_servicos: n.valor_servicos,
        desconto: n.desconto,
        aliquota: n.aliquota,
        iss_retido: false,
        serie: 1,
        numero: 42,
        competencia: n.competencia,
        informacoes: n.informacoes.clone(),
    })
    .unwrap();
    assert_eq!(Nfse::ler_guardada(&antiga).unwrap(), n);
    let mut mei = n.clone();
    mei.opcao_simples = Some(OpcaoSimples::Mei);
    assert_eq!(
        Nfse::ler_guardada(&postcard::to_stdvec(&mei).unwrap()).unwrap(),
        mei
    );
    assert!(Nfse::ler_guardada(b"lixo").is_err());
}

#[test]
fn quem_sai_pela_sefin_nacional() {
    assert!(emite_pela_sefin_nacional("4305108"), "Caxias do Sul");
    assert!(emite_pela_sefin_nacional("43.05108"));
    assert!(
        !emite_pela_sefin_nacional("3550308"),
        "São Paulo tem sistema próprio"
    );
    assert!(
        !emite_pela_sefin_nacional("4205407"),
        "Florianópolis vai pelo Betha"
    );
    assert!(!emite_pela_sefin_nacional("123"));
}

/// Fala de verdade com a produção restrita da Sefin (rede): o certificado de teste não é
/// ICP-Brasil, então a Sefin recusa — o que se confere é que o TLS mútuo sai montado (identidade
/// aceita pelo rustls, servidor verificado) e que a recusa vira uma mensagem legível.
/// `cargo test -p cardeal-fiscal -- --ignored sefin_de_verdade`.
#[test]
#[ignore = "rede: produção restrita da Sefin Nacional"]
fn sefin_de_verdade_recusa_certificado_fora_da_icp() {
    let k = chave();
    let c = cliente::ClienteSefin::novo(&k, AmbienteFiscal::Homologacao).unwrap();
    let id = id_dps("4305108", "11222333000181", 1, 42);
    match c.chave_da_dps(&id) {
        Ok((status, corpo)) => {
            let r = cliente::interpretar_nfse(&(status, corpo));
            eprintln!("Sefin respondeu HTTP {status}: {}", r.motivo());
            assert_eq!(
                status, 403,
                "a renegociação TLS aconteceu e a Sefin avaliou o certificado"
            );
            assert!(r.motivo().contains("recusou o certificado"));
        }
        Err(e) => {
            eprintln!("a Sefin recusou no TLS (esperado para certificado fora da ICP): {e}");
            assert!(matches!(e, ErroFiscal::CertificadoInvalido(_)), "{e:?}");
        }
    }
}

#[test]
fn a_emissao_recua_um_minuto_sem_mudar_de_dia() {
    let as_dez = instante();
    assert_eq!(
        recuar_um_minuto(as_dez).em_micros(),
        as_dez.em_micros() - 60_000_000
    );
    let meia_noite = Instante::de_data_hora(
        Data::de_ymd(2026, 11, 23).unwrap(),
        Hora::de_hms(0, 0, 30).unwrap(),
        Fuso::BRASILIA,
    );
    assert_eq!(recuar_um_minuto(meia_noite), meia_noite);
}
