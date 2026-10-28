use super::*;
use cardeal_fiscal::EnderecoEmitente;
use cardeal_kernel::{Cnpj, Uf};

const NFCE: &str = r#"<nfeProc versao="4.00"><NFe><infNFe Id="NFe43261011444777000161650010000000011000000011" versao="4.00">
<ide><cUF>43</cUF><mod>65</mod><serie>1</serie><nNF>1</nNF><dhEmi>2026-10-03T10:00:00-03:00</dhEmi><tpNF>1</tpNF><finNFe>1</finNFe></ide>
<emit><CNPJ>11444777000161</CNPJ><xNome>Loja Cardeal</xNome><IE>0960012345</IE><CRT>3</CRT></emit>
<det nItem="1"><prod><cProd>P1</cProd><xProd>CAFE</xProd><NCM>09012100</NCM><CFOP>5102</CFOP><uCom>UN</uCom><qCom>2.0000</qCom><vProd>100.00</vProd></prod>
<imposto><ICMS><ICMS00><orig>0</orig><CST>00</CST><vBC>100.00</vBC><pICMS>18.0000</pICMS><vICMS>18.00</vICMS></ICMS00></ICMS></imposto></det>
<det nItem="2"><prod><cProd>P2</cProd><xProd>REFRI</xProd><NCM>22021000</NCM><CFOP>5405</CFOP><uCom>UN</uCom><qCom>5.0000</qCom><vProd>50.00</vProd></prod>
<imposto><ICMS><ICMS60><orig>0</orig><CST>60</CST></ICMS60></ICMS></imposto></det>
<total><ICMSTot><vBC>100.00</vBC><vICMS>18.00</vICMS><vProd>150.00</vProd><vDesc>0.00</vDesc><vNF>150.00</vNF></ICMSTot></total>
<transp><modFrete>9</modFrete></transp></infNFe></NFe><protNFe><infProt><cStat>100</cStat></infProt></protNFe></nfeProc>"#;

const COMPRA: &str = r#"<nfeProc versao="4.00"><NFe><infNFe Id="NFe43261011222333000181550010000004561000004567" versao="4.00">
<ide><mod>55</mod><serie>1</serie><nNF>456</nNF><dhEmi>2026-10-05T09:30:00-03:00</dhEmi><tpNF>1</tpNF><finNFe>1</finNFe></ide>
<emit><CNPJ>11222333000181</CNPJ><xNome>Distribuidora Alfa | LTDA</xNome><enderEmit><xLgr>Rua A</xLgr><nro>10</nro><xBairro>Centro</xBairro><cMun>4314902</cMun><UF>RS</UF></enderEmit><IE>0960000000</IE><CRT>3</CRT></emit>
<dest><CNPJ>11444777000161</CNPJ><xNome>Loja Cardeal</xNome><indIEDest>1</indIEDest></dest>
<det nItem="1"><prod><cProd>A1</cProd><cEAN>7894900011517</cEAN><xProd>REFRI 2L</xProd><NCM>22021000</NCM><CFOP>5405</CFOP><uCom>CX</uCom><qCom>10.0000</qCom><vProd>500.00</vProd><vDesc>20.00</vDesc><vFrete>10.00</vFrete></prod>
<imposto><ICMS><ICMS10><orig>0</orig><CST>10</CST><vBC>490.00</vBC><pICMS>17.0000</pICMS><vICMS>83.30</vICMS><vBCST>686.00</vBCST><pICMSST>17.0000</pICMSST><vICMSST>33.32</vICMSST></ICMS10></ICMS>
<IPI><cEnq>999</cEnq><IPITrib><CST>50</CST><vBC>490.00</vBC><pIPI>5.0000</pIPI><vIPI>24.50</vIPI></IPITrib></IPI></imposto></det>
<det nItem="2"><prod><cProd>B2</cProd><cEAN>SEM GTIN</cEAN><xProd>COPO</xProd><NCM>39241000</NCM><CFOP>5102</CFOP><uCom>UN</uCom><qCom>3</qCom><vProd>30.00</vProd></prod>
<imposto><ICMS><ICMSSN101><orig>0</orig><CSOSN>101</CSOSN><pCredSN>2.5600</pCredSN><vCredICMSSN>0.77</vCredICMSSN></ICMSSN101></ICMS></imposto></det>
<total><ICMSTot><vBC>490.00</vBC><vICMS>83.30</vICMS><vBCST>686.00</vBCST><vST>33.32</vST><vProd>530.00</vProd><vFrete>10.00</vFrete><vDesc>20.00</vDesc><vIPI>24.50</vIPI><vNF>577.82</vNF></ICMSTot></total>
<transp><modFrete>0</modFrete></transp><cobr><dup><nDup>001</nDup><dVenc>2026-11-05</dVenc><vDup>577.82</vDup></dup></cobr>
</infNFe></NFe></nfeProc>"#;

fn emitente(crt: Crt) -> Emitente {
    Emitente {
        cnpj: Cnpj::novo("11.444.777/0001-61").unwrap(),
        razao_social: "Loja Cardeal LTDA".into(),
        nome_fantasia: "Loja Cardeal".into(),
        inscricao_estadual: "096/0012345".into(),
        inscricao_municipal: None,
        crt,
        endereco: EnderecoEmitente {
            logradouro: "Rua das Flores".into(),
            numero: "100".into(),
            complemento: String::new(),
            bairro: "Centro".into(),
            codigo_municipio: "4314902".into(),
            municipio: "Porto Alegre".into(),
            uf: Uf::Rs,
            cep: "90010-000".into(),
            telefone: Some("5133334444".into()),
        },
    }
}

fn configuracao() -> ConfiguracaoEfd {
    ConfiguracaoEfd {
        contabilista: Contabilista {
            nome: "Ana Contadora".into(),
            cpf: "529.982.247-25".into(),
            crc: "RS-012345/O-6".into(),
            email: "ana@contabil.com.br".into(),
            codigo_municipio: "4314902".into(),
            ..Contabilista::default()
        },
        codigo_receita: "0221".into(),
        dia_vencimento: 12,
        ..ConfiguracaoEfd::default()
    }
}

fn cadastro(codigo: &str, unidade: &str) -> ItemDoCadastro {
    ItemDoCadastro {
        codigo: codigo.into(),
        descricao: format!("Produto {codigo}"),
        unidade: unidade.into(),
        ncm: "22021000".into(),
        gtin: None,
        cest: None,
    }
}

fn outubro() -> Competencia {
    Competencia::nova(2026, 10)
}

fn proprias() -> Vec<NotaPropria> {
    vec![
        NotaPropria {
            situacao: SituacaoPropria::Autorizada,
            modelo: ModeloFiscal::Nfce,
            serie: 1,
            numero: 1,
            chave: Some("43261011444777000161650010000000011000000011".into()),
            xml: Some(NFCE.into()),
        },
        NotaPropria {
            situacao: SituacaoPropria::Cancelada,
            modelo: ModeloFiscal::Nfce,
            serie: 1,
            numero: 2,
            chave: Some("43261011444777000161650010000000021000000021".into()),
            xml: None,
        },
    ]
}

fn entradas() -> Vec<EntradaEscriturada> {
    vec![EntradaEscriturada {
        xml: COMPRA.into(),
        data_entrada: Data::de_ymd(2026, 10, 6).unwrap(),
        itens: vec![Some(cadastro("AB12", "UN")), None],
    }]
}

fn gerar(
    crt: Crt,
    saldo_anterior: Dinheiro,
    proprias: &[NotaPropria],
    entradas: &[EntradaEscriturada],
) -> Result<ArquivoEfd, Vec<String>> {
    let e = emitente(crt);
    let c = configuracao();
    gerar_efd(&DadosDaEfd {
        competencia: outubro(),
        substituta: false,
        emitente: &e,
        email: "loja@cardeal.com.br",
        configuracao: &c,
        saldo_credor_anterior: saldo_anterior,
        proprias,
        entradas,
    })
}

fn linhas(a: &ArquivoEfd) -> Vec<Vec<String>> {
    a.texto
        .split("\r\n")
        .filter(|l| !l.is_empty())
        .map(|l| {
            assert!(l.starts_with('|') && l.ends_with('|'), "{l}");
            l[1..l.len() - 1].split('|').map(str::to_owned).collect()
        })
        .collect()
}

fn do_registro<'a>(ls: &'a [Vec<String>], reg: &str) -> Vec<&'a Vec<String>> {
    ls.iter().filter(|l| l[0] == reg).collect()
}

#[test]
fn os_blocos_fecham_com_a_contagem_certa() {
    let a = gerar(Crt::RegimeNormal, Dinheiro::ZERO, &proprias(), &entradas()).unwrap();
    let ls = linhas(&a);
    assert_eq!(
        ls[0],
        [
            "0000",
            "020",
            "0",
            "01102026",
            "31102026",
            "Loja Cardeal LTDA",
            "11444777000161",
            "",
            "RS",
            "0960012345",
            "4314902",
            "",
            "",
            "A",
            "1"
        ]
    );
    // Cada X990 conta as linhas do bloco X (do X001 ao X990).
    for bloco in ["0", "B", "C", "D", "E", "G", "H", "K", "1"] {
        let quantas = ls.iter().filter(|l| l[0].starts_with(bloco)).count();
        let fecho = do_registro(&ls, &format!("{bloco}990"));
        assert_eq!(fecho[0][1], quantas.to_string(), "bloco {bloco}");
    }
    // O 9900 de cada registro bate com o arquivo; o 9990 e o 9999 fecham.
    for l in do_registro(&ls, "9900") {
        let n = ls.iter().filter(|x| x[0] == l[1]).count();
        assert_eq!(l[2], n.to_string(), "9900 do {}", l[1]);
    }
    let nove = ls.iter().filter(|l| l[0].starts_with('9')).count();
    assert_eq!(do_registro(&ls, "9990")[0][1], nove.to_string());
    assert_eq!(do_registro(&ls, "9999")[0][1], ls.len().to_string());
    // Quantidade de campos de cada registro, como no leiaute.
    for (reg, campos) in [
        ("0000", 15),
        ("0005", 10),
        ("0100", 14),
        ("0150", 13),
        ("0190", 3),
        ("0200", 13),
        ("C100", 29),
        ("C170", 38),
        ("C190", 12),
        ("E110", 15),
        ("1010", 14),
    ] {
        for l in do_registro(&ls, reg) {
            assert_eq!(l.len(), campos, "{reg}: {l:?}");
        }
    }
    assert_eq!(a.documentos, 3);
    assert_eq!(a.nome, "EFD_ICMS_IPI_11444777000161_202610.txt");
}

#[test]
fn a_nfce_vai_sem_participante_nem_st_ipi_pis_cofins_e_a_cancelada_so_com_a_chave() {
    let a = gerar(Crt::RegimeNormal, Dinheiro::ZERO, &proprias(), &[]).unwrap();
    let ls = linhas(&a);
    let c100 = do_registro(&ls, "C100");
    let nfce = c100[0];
    assert_eq!((nfce[1].as_str(), nfce[2].as_str()), ("1", "0"));
    assert_eq!(nfce[3], "", "sem participante");
    assert_eq!((nfce[4].as_str(), nfce[5].as_str()), ("65", "00"));
    assert_eq!(nfce[10], "", "sem DT_E_S");
    assert_eq!(nfce[11], "150,00");
    assert_eq!((nfce[20].as_str(), nfce[21].as_str()), ("100,00", "18,00"));
    assert!(nfce[22..].iter().all(String::is_empty), "{nfce:?}");
    // A nota própria não tem C170: o C190 vem logo depois.
    let c190 = do_registro(&ls, "C190");
    assert_eq!(
        c190[0][1..7],
        ["000", "5102", "18,00", "100,00", "100,00", "18,00"]
    );
    assert_eq!(c190[1][1..5], ["060", "5405", "0,00", "50,00"]);
    assert!(do_registro(&ls, "C170").is_empty());

    let cancelada = c100[1];
    assert_eq!(cancelada[5], "02");
    assert_eq!(cancelada[8], "43261011444777000161650010000000021000000021");
    assert!(cancelada[9..].iter().all(String::is_empty));
    assert!(do_registro(&ls, "0150").is_empty());
}

#[test]
fn a_compra_vira_entrada_pela_otica_de_quem_compra() {
    let a = gerar(Crt::RegimeNormal, Dinheiro::ZERO, &[], &entradas()).unwrap();
    let ls = linhas(&a);
    let c100 = do_registro(&ls, "C100")[0];
    assert_eq!(c100[1..6], ["0", "1", "11222333000181", "55", "00"]);
    assert_eq!(c100[9..13], ["05102026", "06102026", "577,82", "1"]);
    assert_eq!(c100[16], "0", "frete por conta do emitente");
    // O ICMS do C100 é o crédito somado (83,30 + 0,77 do Simples).
    assert_eq!(c100[21], "84,07");
    assert_eq!((c100[23].as_str(), c100[24].as_str()), ("33,32", "24,50"));

    let c170 = do_registro(&ls, "C170");
    assert_eq!(c170.len(), 2);
    // O produto casado: o código e a unidade do cadastro; o CFOP de entrada com ST.
    assert_eq!(c170[0][1..7], ["1", "AB12", "", "10,00000", "UN", "500,00"]);
    assert_eq!(c170[0][9..11], ["010", "1403"]);
    assert_eq!(c170[0][12..15], ["490,00", "17,00", "83,30"]);
    assert_eq!(c170[0][19], "49", "IPI só informado");
    assert_eq!(c170[0][23], "24,50");
    // O item sem casamento vai com o código do fornecedor; o CSOSN 101 vira CST 090 com o
    // crédito permitido.
    assert_eq!(c170[1][2], "11222333000181-B2");
    assert_eq!(c170[1][9..11], ["090", "1102"]);
    assert_eq!(c170[1][12..15], ["30,00", "2,56", "0,77"]);
    assert!(a
        .avisos
        .iter()
        .any(|x| x.contains("item 2: sem produto casado")));

    let c190 = do_registro(&ls, "C190");
    let total: i64 = c190
        .iter()
        .map(|l| l[4].replace(',', "").parse::<i64>().unwrap())
        .sum();
    assert_eq!(total, 57_782, "o C190 soma o valor do documento");

    // O participante sem o "|" do nome; o item e a unidade no bloco 0.
    let p = do_registro(&ls, "0150")[0];
    assert_eq!(
        p[1..5],
        [
            "11222333000181",
            "Distribuidora Alfa   LTDA",
            "01058",
            "11222333000181"
        ]
    );
    assert_eq!(p[7], "4314902");
    let itens: Vec<&str> = do_registro(&ls, "0200")
        .iter()
        .map(|l| l[1].as_str())
        .collect();
    assert_eq!(itens, ["11222333000181-B2", "AB12"]);
    let unidades: Vec<&str> = do_registro(&ls, "0190")
        .iter()
        .map(|l| l[1].as_str())
        .collect();
    assert_eq!(unidades, ["UN"]);
}

#[test]
fn a_apuracao_soma_debitos_e_creditos_e_leva_o_saldo_credor() {
    let a = gerar(
        Crt::RegimeNormal,
        Dinheiro::reais(10),
        &proprias(),
        &entradas(),
    )
    .unwrap();
    assert_eq!(a.apuracao.debitos, Dinheiro::reais(18));
    assert_eq!(a.apuracao.creditos, Dinheiro::centavos(8_407));
    // 18 − 84,07 − 10 = −76,07: nada a recolher, 76,07 para novembro.
    assert_eq!(a.apuracao.a_recolher, Dinheiro::ZERO);
    assert_eq!(
        a.apuracao.saldo_credor_a_transportar,
        Dinheiro::centavos(7_607)
    );
    let ls = linhas(&a);
    let e110 = do_registro(&ls, "E110")[0];
    assert_eq!(e110[1], "18,00");
    assert_eq!(e110[5], "84,07");
    assert_eq!(e110[9], "10,00");
    assert_eq!(e110[13], "76,07");
    assert!(do_registro(&ls, "E116").is_empty());
}

#[test]
fn com_imposto_a_recolher_sai_a_obrigacao_com_vencimento_e_codigo() {
    let a = gerar(Crt::RegimeNormal, Dinheiro::reais(5), &proprias(), &[]).unwrap();
    assert_eq!(a.apuracao.a_recolher, Dinheiro::reais(13));
    let ls = linhas(&a);
    let e116 = do_registro(&ls, "E116")[0];
    assert_eq!(
        e116[1..],
        ["000", "13,00", "12112026", "0221", "", "", "", "", "102026"]
    );
}

#[test]
fn o_simples_e_a_configuracao_incompleta_impedem() {
    let e = gerar(Crt::SimplesNacional, Dinheiro::ZERO, &[], &[]).unwrap_err();
    assert!(e[0].contains("Simples Nacional"));
    let mut c = configuracao();
    c.contabilista.cpf = "123".into();
    c.industrial = true;
    let f = c.pendencias();
    assert!(f.iter().any(|x| x.contains("CPF")));
    assert!(f.iter().any(|x| x.contains("industrial")));
}

#[test]
fn sem_xml_a_nota_autorizada_fica_fora_com_aviso() {
    let mut p = proprias();
    p[0].xml = None;
    let a = gerar(Crt::RegimeNormal, Dinheiro::ZERO, &p, &[]).unwrap();
    assert_eq!(a.documentos, 1, "só a cancelada");
    assert!(a.avisos[0].contains("sem o XML guardado"));
}

#[test]
fn cfop_de_entrada_pela_operacao() {
    assert_eq!(cfop_de_entrada("5102", false), "1102");
    assert_eq!(cfop_de_entrada("6102", false), "2102");
    assert_eq!(cfop_de_entrada("5101", true), "1101");
    assert_eq!(cfop_de_entrada("5405", false), "1403");
    assert_eq!(cfop_de_entrada("6403", true), "2401");
    assert_eq!(cfop_de_entrada("5910", false), "1910");
    assert_eq!(cfop_de_entrada("5202", false), "1202");
}

#[test]
fn o_arquivo_sai_em_latin1() {
    let a = ArquivoEfd {
        nome: String::new(),
        texto: "Ação €".into(),
        apuracao: ApuracaoIcms::default(),
        documentos: 0,
        avisos: Vec::new(),
    };
    assert_eq!(a.em_latin1(), [b'A', 0xE7, 0xE3, b'o', b' ', b'?']);
}
