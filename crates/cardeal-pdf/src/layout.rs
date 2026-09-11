//! O layout do orçamento profissional em A4 — cabeçalho com a marca, tabela de itens com
//! paginação, quadro de totais, condições e rodapé de aceite em toda página.

use cardeal_kernel::Instante;

use crate::documento::{Canvas, Cor, Documento, MM};
use crate::fonte::{largura_texto, Fonte};
use crate::imagem::decodificar_logo;
use crate::{ComprovanteOsPdf, ErroPdf, IdentidadeEmpresa, OrcamentoPdf};

// ── Paleta (espelha os tokens do design system Rubro) ─────────────────────────
const TEXTO_FORTE: Cor = Cor::rgb(17, 17, 20);
const TEXTO: Cor = Cor::rgb(39, 39, 42);
const TEXTO_MEDIO: Cor = Cor::rgb(82, 82, 91);
const TEXTO_FRACO: Cor = Cor::rgb(138, 138, 147);
const BORDA: Cor = Cor::rgb(224, 224, 228);
const ZEBRA: Cor = Cor::rgb(249, 249, 250);
const CABECALHO_TABELA: Cor = Cor::rgb(243, 244, 246);
const RUBRO: Cor = Cor::rgb(239, 68, 59);

// ── Geometria (mm) ───────────────────────────────────────────────────────────
const MX: f32 = 18.0;
const DIR: f32 = 192.0; // 210 - 18
const LARGURA_CONTEUDO: f32 = DIR - MX;
const TOPO: f32 = 15.0;
const LIMITE_INFERIOR: f32 = 275.0; // abaixo disto começa nova página

// Colunas da tabela.
const COL_NUM: f32 = MX;
const COL_DESC: f32 = 25.0;
const COL_DESC_LARG: f32 = 90.0;
const COL_QTD_DIR: f32 = 122.0;
const COL_UN: f32 = 125.0;
const COL_UNIT_DIR: f32 = 152.0;
const COL_DESCP_DIR: f32 = 170.0;
const COL_TOTAL_DIR: f32 = DIR;

struct Compositor<'a> {
    paginas: Vec<Canvas>,
    y: f32,
    logo: Option<(usize, f32)>, // (índice da imagem, proporção largura/altura)
    empresa: &'a IdentidadeEmpresa,
    gerado_em: String,
}

/// Gera o PDF do orçamento.
///
/// # Errors
/// [`ErroPdf::Logo`] se a logo cadastrada não for uma imagem válida.
pub fn gerar(
    empresa: &IdentidadeEmpresa,
    orc: &OrcamentoPdf,
    gerado_em: Instante,
) -> Result<Vec<u8>, ErroPdf> {
    let mut doc = Documento::novo();
    let logo = match &empresa.logo_png {
        Some(bytes) if !bytes.is_empty() => {
            let img = decodificar_logo(bytes)?;
            let prop = img.proporcao();
            Some((doc.adicionar_imagem(img), prop))
        }
        _ => None,
    };

    let mut c = Compositor {
        paginas: vec![Canvas::nova_a4()],
        y: TOPO,
        logo,
        empresa,
        gerado_em: gerado_em.formatar(cardeal_kernel::Fuso::BRASILIA),
    };

    c.cabecalho_marca();
    c.bloco_titulo(orc);
    c.bloco_cliente(orc);
    c.bloco_assunto(orc);
    c.tabela_itens(&orc.itens);
    c.quadro_totais(orc.subtotal, orc.desconto, orc.total);
    c.bloco_condicoes(orc);
    c.aceite();
    c.rodape_em_todas(&orc.responsavel);

    for pagina in c.paginas {
        doc.pagina_bruta(pagina.em_operadores());
    }
    Ok(doc.finalizar())
}

/// Gera o PDF do comprovante de Ordem de Serviço — mesmo motor de composição de [`gerar`]
/// (cabeçalho, tabela de itens, totais, aceite são literalmente os mesmos métodos); só o bloco
/// de identificação (cliente + equipamento + diagnóstico + garantia) é próprio deste documento.
///
/// # Errors
/// [`ErroPdf::Logo`] se a logo cadastrada não for uma imagem válida.
pub fn gerar_os(
    empresa: &IdentidadeEmpresa,
    os: &ComprovanteOsPdf,
    gerado_em: Instante,
) -> Result<Vec<u8>, ErroPdf> {
    let mut doc = Documento::novo();
    let logo = match &empresa.logo_png {
        Some(bytes) if !bytes.is_empty() => {
            let img = decodificar_logo(bytes)?;
            let prop = img.proporcao();
            Some((doc.adicionar_imagem(img), prop))
        }
        _ => None,
    };

    let mut c = Compositor {
        paginas: vec![Canvas::nova_a4()],
        y: TOPO,
        logo,
        empresa,
        gerado_em: gerado_em.formatar(cardeal_kernel::Fuso::BRASILIA),
    };

    c.cabecalho_marca();
    c.bloco_titulo_os(os);
    c.bloco_cliente_equipamento(os);
    if !os.defeito_relatado.is_empty() {
        c.bloco_texto("DEFEITO RELATADO", &os.defeito_relatado);
    }
    if !os.diagnostico.is_empty() {
        c.bloco_texto("DIAGNÓSTICO TÉCNICO", &os.diagnostico);
    }
    if !os.itens.is_empty() {
        c.tabela_itens(&os.itens);
        c.quadro_totais(os.subtotal, cardeal_kernel::Dinheiro::ZERO, os.total);
    }
    c.bloco_garantia_e_aprovacao(os);
    c.aceite();
    c.rodape_em_todas(&os.tecnico_responsavel);

    for pagina in c.paginas {
        doc.pagina_bruta(pagina.em_operadores());
    }
    Ok(doc.finalizar())
}

impl Compositor<'_> {
    fn canvas(&mut self) -> &mut Canvas {
        self.paginas.last_mut().expect("sempre há uma página")
    }

    fn nova_pagina(&mut self) {
        self.paginas.push(Canvas::nova_a4());
        self.y = TOPO;
    }

    /// Garante `altura` mm livres antes do limite inferior; abre página nova se preciso.
    fn garantir(&mut self, altura: f32) {
        if self.y + altura > LIMITE_INFERIOR {
            self.nova_pagina();
        }
    }

    fn texto_em(&mut self, x: f32, y: f32, fonte: Fonte, tam: f32, cor: Cor, s: &str) {
        self.canvas().texto(x, y, fonte, tam, cor, s);
    }

    fn texto_direita(&mut self, x_dir: f32, y: f32, fonte: Fonte, tam: f32, cor: Cor, s: &str) {
        let x = x_dir - largura_texto(s, fonte, tam) / MM;
        self.canvas().texto(x, y, fonte, tam, cor, s);
    }

    // ── Cabeçalho com a marca (só na 1ª página) ──────────────────────────────
    fn cabecalho_marca(&mut self) {
        let e = self.empresa;
        let topo_texto = TOPO + 3.0;

        if let Some((idx, prop)) = self.logo {
            let alt = 16.0_f32;
            let larg = (alt * prop).min(55.0);
            let alt = if larg >= alt * prop { alt } else { larg / prop };
            self.canvas().imagem(idx, MX, TOPO, larg, alt);
        }

        // Bloco de identidade à direita, empilhado.
        let mut y = topo_texto;
        let nome = if e.nome_fantasia.is_empty() {
            e.razao_social.as_str()
        } else {
            e.nome_fantasia.as_str()
        };
        self.texto_direita(DIR, y, Fonte::Negrito, 12.0, TEXTO_FORTE, nome);
        y += 4.6;
        if !e.nome_fantasia.is_empty()
            && !e.razao_social.is_empty()
            && e.razao_social != e.nome_fantasia
        {
            self.texto_direita(DIR, y, Fonte::Regular, 8.0, TEXTO_MEDIO, &e.razao_social);
            y += 3.8;
        }
        if !e.cnpj.is_empty() {
            self.texto_direita(
                DIR,
                y,
                Fonte::Regular,
                8.0,
                TEXTO_MEDIO,
                &format!("CNPJ {}", e.cnpj),
            );
            y += 3.8;
        }
        if !e.endereco.is_empty() {
            self.texto_direita(DIR, y, Fonte::Regular, 8.0, TEXTO_MEDIO, &e.endereco);
            y += 3.8;
        }
        let contato = [e.telefone.as_str(), e.email.as_str(), e.site.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("   ·   ");
        if !contato.is_empty() {
            self.texto_direita(DIR, y, Fonte::Regular, 8.0, TEXTO_MEDIO, &contato);
            y += 3.8;
        }

        let base = y.max(topo_texto + 16.0);
        self.canvas().linha(MX, base, DIR, base, 1.2, RUBRO);
        self.y = base + 9.0;
    }

    // ── Título ──────────────────────────────────────────────────────────────
    fn bloco_titulo(&mut self, orc: &OrcamentoPdf) {
        let y = self.y;
        self.texto_em(
            MX,
            y,
            Fonte::Negrito,
            17.0,
            TEXTO_FORTE,
            &format!("Orçamento Nº {:04}", orc.numero),
        );
        self.texto_direita(
            DIR,
            y - 3.5,
            Fonte::Regular,
            9.0,
            TEXTO_MEDIO,
            &format!("Emissão  {}", orc.data_emissao.formatar()),
        );
        self.texto_direita(
            DIR,
            y + 1.0,
            Fonte::Negrito,
            9.0,
            RUBRO,
            &format!("Válido até  {}", orc.validade.formatar()),
        );
        self.y = y + 9.0;
    }

    // ── Cliente ─────────────────────────────────────────────────────────────
    fn bloco_cliente(&mut self, orc: &OrcamentoPdf) {
        self.rotulo("CLIENTE");
        self.y += 4.6;
        let y = self.y;
        self.texto_em(MX, y, Fonte::Negrito, 10.5, TEXTO_FORTE, &orc.cliente_nome);
        let mut linha2: Vec<String> = Vec::new();
        if !orc.cliente_documento.is_empty() {
            linha2.push(orc.cliente_documento.clone());
        }
        if !orc.cliente_contato.is_empty() {
            linha2.push(orc.cliente_contato.clone());
        }
        if !linha2.is_empty() {
            self.texto_em(
                MX,
                y + 4.4,
                Fonte::Regular,
                9.0,
                TEXTO_MEDIO,
                &linha2.join("   ·   "),
            );
            self.y = y + 4.4;
        }
        self.y += 8.0;
    }

    // ── Assunto + descrição ─────────────────────────────────────────────────
    fn bloco_assunto(&mut self, orc: &OrcamentoPdf) {
        if orc.assunto.is_empty() && orc.descricao.is_empty() {
            return;
        }
        self.rotulo("ASSUNTO");
        self.y += 4.6;
        if !orc.assunto.is_empty() {
            for linha in quebrar(&orc.assunto, Fonte::Regular, 10.5, LARGURA_CONTEUDO) {
                let y = self.y;
                self.texto_em(MX, y, Fonte::Regular, 10.5, TEXTO, &linha);
                self.y += 4.8;
            }
        }
        if !orc.descricao.is_empty() {
            self.y += 1.0;
            for linha in quebrar(&orc.descricao, Fonte::Regular, 9.0, LARGURA_CONTEUDO) {
                self.garantir(6.0);
                let y = self.y;
                self.texto_em(MX, y, Fonte::Regular, 9.0, TEXTO_MEDIO, &linha);
                self.y += 4.2;
            }
        }
        self.y += 6.0;
    }

    // ── Tabela de itens (compartilhada entre orçamento e comprovante de OS) ──
    pub(crate) fn tabela_itens(&mut self, itens: &[crate::ItemPdf]) {
        self.garantir(18.0);
        self.cabecalho_tabela();

        for (i, item) in itens.iter().enumerate() {
            let linhas_desc = quebrar(&item.descricao, Fonte::Regular, 9.0, COL_DESC_LARG);
            let altura = (linhas_desc.len() as f32 * 4.4).max(7.0) + 1.6;

            if self.y + altura > LIMITE_INFERIOR {
                self.nova_pagina();
                self.cabecalho_tabela();
            }

            if i % 2 == 1 {
                let y = self.y;
                self.canvas()
                    .retangulo(MX, y - 1.0, LARGURA_CONTEUDO, altura, ZEBRA);
            }

            let base = self.y + 3.6;
            self.texto_em(
                COL_NUM,
                base,
                Fonte::Regular,
                8.5,
                TEXTO_FRACO,
                &format!("{:02}", i + 1),
            );
            for (k, linha) in linhas_desc.iter().enumerate() {
                self.texto_em(
                    COL_DESC,
                    base + k as f32 * 4.4,
                    Fonte::Regular,
                    9.0,
                    TEXTO,
                    linha,
                );
            }
            self.texto_direita(
                COL_QTD_DIR,
                base,
                Fonte::Regular,
                9.0,
                TEXTO,
                &item.quantidade.formatar(0),
            );
            if !item.unidade.is_empty() {
                self.texto_em(
                    COL_UN,
                    base,
                    Fonte::Regular,
                    9.0,
                    TEXTO_MEDIO,
                    &item.unidade,
                );
            }
            self.texto_direita(
                COL_UNIT_DIR,
                base,
                Fonte::Regular,
                9.0,
                TEXTO,
                &item.preco_unitario.formatar(),
            );
            let desc = if item.desconto_pct.e_zero() {
                "—".to_owned()
            } else {
                format!("{}%", item.desconto_pct.formatar(1))
            };
            self.texto_direita(COL_DESCP_DIR, base, Fonte::Regular, 9.0, TEXTO_MEDIO, &desc);
            self.texto_direita(
                COL_TOTAL_DIR,
                base,
                Fonte::Negrito,
                9.0,
                TEXTO_FORTE,
                &item.total.formatar(),
            );

            self.y += altura;
            let y = self.y;
            self.canvas().linha(MX, y, DIR, y, 0.4, BORDA);
        }
        self.y += 2.0;
    }

    fn cabecalho_tabela(&mut self) {
        let y = self.y;
        self.canvas()
            .retangulo(MX, y, LARGURA_CONTEUDO, 7.0, CABECALHO_TABELA);
        let base = y + 4.7;
        self.texto_em(COL_NUM, base, Fonte::Negrito, 7.5, TEXTO_MEDIO, "#");
        self.texto_em(
            COL_DESC,
            base,
            Fonte::Negrito,
            7.5,
            TEXTO_MEDIO,
            "DESCRIÇÃO",
        );
        self.texto_direita(COL_QTD_DIR, base, Fonte::Negrito, 7.5, TEXTO_MEDIO, "QTD");
        self.texto_em(COL_UN, base, Fonte::Negrito, 7.5, TEXTO_MEDIO, "UN");
        self.texto_direita(
            COL_UNIT_DIR,
            base,
            Fonte::Negrito,
            7.5,
            TEXTO_MEDIO,
            "VL UNIT",
        );
        self.texto_direita(
            COL_DESCP_DIR,
            base,
            Fonte::Negrito,
            7.5,
            TEXTO_MEDIO,
            "DESC",
        );
        self.texto_direita(
            COL_TOTAL_DIR,
            base,
            Fonte::Negrito,
            7.5,
            TEXTO_MEDIO,
            "TOTAL",
        );
        self.y = y + 8.6;
    }

    // ── Quadro de totais (compartilhado) ────────────────────────────────────
    pub(crate) fn quadro_totais(
        &mut self,
        subtotal: cardeal_kernel::Dinheiro,
        desconto: cardeal_kernel::Dinheiro,
        total: cardeal_kernel::Dinheiro,
    ) {
        self.garantir(26.0);
        let x = DIR - 78.0;
        let mut y = self.y + 2.0;

        y = self.linha_total(x, y, "Subtotal", &subtotal.formatar_com_simbolo(), false);
        if !desconto.e_zero() {
            y = self.linha_total(
                x,
                y,
                "Desconto",
                &format!("- {}", desconto.formatar_com_simbolo()),
                false,
            );
        }
        self.canvas().linha(x, y - 2.0, DIR, y - 2.0, 0.6, BORDA);
        y += 1.0;
        y = self.linha_total(x, y, "TOTAL", &total.formatar_com_simbolo(), true);

        self.y = y + 6.0;
    }

    fn linha_total(&mut self, x: f32, y: f32, rot: &str, val: &str, destaque: bool) -> f32 {
        let (peso, cor, tam) = if destaque {
            (Fonte::Negrito, TEXTO_FORTE, 11.0)
        } else {
            (Fonte::Regular, TEXTO_MEDIO, 9.5)
        };
        self.texto_em(x, y, peso, tam, cor, rot);
        self.texto_direita(DIR, y, peso, tam, if destaque { RUBRO } else { TEXTO }, val);
        y + if destaque { 7.0 } else { 5.4 }
    }

    // ── Condições ───────────────────────────────────────────────────────────
    fn bloco_condicoes(&mut self, orc: &OrcamentoPdf) {
        let secoes = [
            ("CONDIÇÕES DE PAGAMENTO", &orc.condicoes_pagamento),
            ("PRAZO DE ENTREGA", &orc.prazo_entrega),
            ("OBSERVAÇÕES", &orc.observacoes),
        ];
        for (rot, texto) in secoes {
            if texto.is_empty() {
                continue;
            }
            self.garantir(14.0);
            self.rotulo(rot);
            self.y += 4.4;
            for linha in quebrar(texto, Fonte::Regular, 9.0, LARGURA_CONTEUDO) {
                self.garantir(6.0);
                let y = self.y;
                self.texto_em(MX, y, Fonte::Regular, 9.0, TEXTO, &linha);
                self.y += 4.4;
            }
            self.y += 4.0;
        }
    }

    // ── Título — comprovante de OS ────────────────────────────────────────
    fn bloco_titulo_os(&mut self, os: &ComprovanteOsPdf) {
        let y = self.y;
        self.texto_em(
            MX,
            y,
            Fonte::Negrito,
            17.0,
            TEXTO_FORTE,
            &format!("Ordem de Serviço Nº {:04}", os.numero),
        );
        self.texto_direita(
            DIR,
            y - 3.5,
            Fonte::Regular,
            9.0,
            TEXTO_MEDIO,
            &format!("Abertura  {}", os.data_abertura.formatar()),
        );
        self.texto_direita(DIR, y + 1.0, Fonte::Negrito, 9.0, RUBRO, &os.situacao);
        self.y = y + 9.0;
    }

    // ── Cliente + equipamento — comprovante de OS ────────────────────────
    fn bloco_cliente_equipamento(&mut self, os: &ComprovanteOsPdf) {
        self.rotulo("CLIENTE");
        self.y += 4.6;
        let y = self.y;
        self.texto_em(MX, y, Fonte::Negrito, 10.5, TEXTO_FORTE, &os.cliente_nome);
        let mut linha2: Vec<String> = Vec::new();
        if !os.cliente_documento.is_empty() {
            linha2.push(os.cliente_documento.clone());
        }
        if !os.cliente_contato.is_empty() {
            linha2.push(os.cliente_contato.clone());
        }
        if !linha2.is_empty() {
            self.texto_em(
                MX,
                y + 4.4,
                Fonte::Regular,
                9.0,
                TEXTO_MEDIO,
                &linha2.join("   ·   "),
            );
            self.y = y + 4.4;
        }
        self.y += 8.0;

        self.rotulo("EQUIPAMENTO");
        self.y += 4.6;
        let y = self.y;
        self.texto_em(MX, y, Fonte::Negrito, 10.5, TEXTO_FORTE, &os.equipamento);
        self.y = y + 8.0;
    }

    /// Bloco de texto livre com quebra de linha — reusado por DEFEITO RELATADO e
    /// DIAGNÓSTICO TÉCNICO no comprovante de OS (mesmo estilo de [`Self::bloco_assunto`]).
    fn bloco_texto(&mut self, rotulo: &str, texto: &str) {
        self.garantir(14.0);
        self.rotulo(rotulo);
        self.y += 4.6;
        for linha in quebrar(texto, Fonte::Regular, 9.5, LARGURA_CONTEUDO) {
            self.garantir(6.0);
            let y = self.y;
            self.texto_em(MX, y, Fonte::Regular, 9.5, TEXTO, &linha);
            self.y += 4.6;
        }
        self.y += 6.0;
    }

    // ── Garantia + aprovação — comprovante de OS ─────────────────────────
    fn bloco_garantia_e_aprovacao(&mut self, os: &ComprovanteOsPdf) {
        self.garantir(16.0);
        self.rotulo("GARANTIA");
        self.y += 4.6;
        let y = self.y;
        let texto_garantia = if os.garantia_dias > 0 {
            format!(
                "{} dias sobre o serviço executado, a partir da entrega do equipamento.",
                os.garantia_dias
            )
        } else {
            "Sem garantia sobre este serviço.".to_owned()
        };
        self.texto_em(MX, y, Fonte::Regular, 9.5, TEXTO, &texto_garantia);
        self.y = y + 6.0;

        if !os.aprovado_por.is_empty() {
            self.garantir(10.0);
            self.rotulo("ORÇAMENTO APROVADO POR");
            self.y += 4.6;
            let y = self.y;
            self.texto_em(MX, y, Fonte::Regular, 9.5, TEXTO, &os.aprovado_por);
            self.y = y + 6.0;
        }
    }

    // ── Aceite (última página) ──────────────────────────────────────────────
    fn aceite(&mut self) {
        self.garantir(24.0);
        self.y += 6.0;
        let y = self.y;
        self.canvas().linha(MX, y, MX + 80.0, y, 0.6, TEXTO_FRACO);
        self.canvas().linha(DIR - 55.0, y, DIR, y, 0.6, TEXTO_FRACO);
        self.texto_em(
            MX,
            y + 4.0,
            Fonte::Regular,
            8.0,
            TEXTO_MEDIO,
            "Assinatura — de acordo",
        );
        self.texto_em(
            DIR - 55.0,
            y + 4.0,
            Fonte::Regular,
            8.0,
            TEXTO_MEDIO,
            "Data",
        );
    }

    // ── Rodapé em toda página (compartilhado) ───────────────────────────────
    pub(crate) fn rodape_em_todas(&mut self, responsavel: &str) {
        let total = self.paginas.len();
        let responsavel = if responsavel.is_empty() {
            String::new()
        } else {
            format!("  ·  Responsável: {responsavel}")
        };
        let esquerda = format!("Gerado pelo Cardeal em {}{}", self.gerado_em, responsavel);
        for (i, pagina) in self.paginas.iter_mut().enumerate() {
            let y = 288.0;
            pagina.linha(MX, y - 4.0, DIR, y - 4.0, 0.4, BORDA);
            pagina.texto(MX, y, Fonte::Regular, 7.5, TEXTO_FRACO, &esquerda);
            let dir_txt = format!("Página {} de {}", i + 1, total);
            let x = DIR - largura_texto(&dir_txt, Fonte::Regular, 7.5) / MM;
            pagina.texto(x, y, Fonte::Regular, 7.5, TEXTO_FRACO, &dir_txt);
        }
    }

    fn rotulo(&mut self, texto: &str) {
        let y = self.y;
        self.canvas()
            .texto(MX, y, Fonte::Negrito, 7.0, TEXTO_FRACO, texto);
    }
}

/// Quebra `texto` em linhas que cabem em `larg_mm`, respeitando quebras explícitas.
fn quebrar(texto: &str, fonte: Fonte, tam: f32, larg_mm: f32) -> Vec<String> {
    let larg_pt = larg_mm * MM;
    let mut linhas = Vec::new();
    for paragrafo in texto.split('\n') {
        let mut atual = String::new();
        for palavra in paragrafo.split_whitespace() {
            let candidato = if atual.is_empty() {
                palavra.to_owned()
            } else {
                format!("{atual} {palavra}")
            };
            if largura_texto(&candidato, fonte, tam) <= larg_pt || atual.is_empty() {
                atual = candidato;
            } else {
                linhas.push(std::mem::take(&mut atual));
                atual.push_str(palavra);
            }
        }
        linhas.push(atual);
    }
    if linhas.is_empty() {
        linhas.push(String::new());
    }
    linhas
}
