//! O topo do detalhe da OS: quem é o cliente (com o WhatsApp a um clique, e a mensagem certa
//! para o estado da OS), o que foi anotado na recepção e as outras OS do mesmo aparelho.

use super::*;

/// Desenha o bloco de resumo.
pub(super) fn secao_resumo(ui: &mut egui::Ui, estado: &mut EstadoTelaOs, detalhe: &DetalheOrdem) {
    let os = &detalhe.ordem;
    let cliente = estado.clientes.iter().find(|c| c.pessoa == os.cliente);
    let hoje = Data::hoje(Fuso::BRASILIA);

    ui.horizontal(|ui| {
        ui.add(Rotulo::campo("Cliente"));
        ui.add(Rotulo::interface(
            cliente.map_or("Cliente", |c| c.nome.as_str()).to_owned(),
        ));
        if let Some(tel) = cliente.and_then(|c| c.telefone.as_deref()) {
            ui.add(Rotulo::interface(formatar_telefone(tel)).cor(ui.cores().texto_medio));
            if let Some(link) = link_whatsapp(tel, &mensagem_whatsapp(estado, detalhe)) {
                if ui.add(Botao::secundario("WhatsApp").pequeno()).clicked() {
                    if let Err(e) = open::that_detached(&link) {
                        notificar(
                            ui.ctx(),
                            Notificacao::erro("Não foi possível abrir o WhatsApp")
                                .detalhe(e.to_string()),
                        );
                    }
                }
            }
        }
    });

    let ficha = &os.ficha;
    if ficha.previsao_entrega.is_some()
        || !ficha.numero_serie.is_empty()
        || !ficha.acessorios.is_empty()
    {
        ui.add_space(Espaco::E4);
        ui.horizontal_wrapped(|ui| {
            if let Some(e) = etiqueta_previsao(os, hoje) {
                ui.add(Rotulo::campo("Previsão"));
                ui.add(e);
            }
            if !ficha.numero_serie.is_empty() {
                ui.add(Rotulo::campo("Série/IMEI"));
                ui.add(Rotulo::interface(ficha.numero_serie.clone()));
            }
        });
        if !ficha.acessorios.is_empty() {
            ui.add(
                Rotulo::interface(format!("Entrada: {}", ficha.acessorios))
                    .quebravel()
                    .cor(ui.cores().texto_medio),
            );
        }
    }

    if !estado.historico.is_empty() {
        ui.add_space(Espaco::E8);
        let titulo = format!("Outras OS deste aparelho ({})", estado.historico.len());
        let historico = estado.historico.clone();
        let mut abrir = None;
        SecaoExpansivel::nova(titulo).mostrar(ui, |ui| {
            for h in &historico {
                ui.horizontal(|ui| {
                    let (rotulo, tom) = estado_etiqueta(h.estado);
                    if ui
                        .add(Botao::fantasma(format!("OS #{}", h.numero)).pequeno())
                        .clicked()
                    {
                        abrir = Some(h.id);
                    }
                    ui.add(Rotulo::interface(format!(
                        "{} · {} · {}",
                        h.data_abertura.formatar(),
                        h.equipamento,
                        h.defeito_relatado
                    )));
                    ui.add(Etiqueta::nova(rotulo, tom));
                    if em_garantia(h, hoje) {
                        ui.add(Etiqueta::atencao("em garantia"));
                    }
                });
            }
        });
        if let Some(id) = abrir {
            estado.pedido_abrir = Some(id);
        }
    }
    ui.add_space(Espaco::E12);
}

/// Uma OS faturada cuja garantia certamente ainda vale hoje. Conta a partir da abertura (a
/// data de faturamento não fica na OS): como faturar vem depois de abrir, o prazo real só pode
/// terminar depois — então "sim" aqui nunca é falso positivo.
pub(super) fn em_garantia(os: &OrdemServico, hoje: Data) -> bool {
    os.estado == EstadoOs::Faturada
        && os.garantia_dias > 0
        && os.data_abertura.mais_dias(i32::from(os.garantia_dias)) >= hoje
}

/// O texto que abre no WhatsApp, conforme o ponto em que a OS está.
pub(super) fn mensagem_whatsapp(estado: &EstadoTelaOs, detalhe: &DetalheOrdem) -> String {
    let os = &detalhe.ordem;
    let nome = estado
        .clientes
        .iter()
        .find(|c| c.pessoa == os.cliente)
        .map_or(String::new(), |c| {
            c.nome.split_whitespace().next().unwrap_or("").to_owned()
        });
    let saudacao = if nome.is_empty() {
        "Olá!".to_owned()
    } else {
        format!("Olá, {nome}!")
    };
    let aparelho = format!("seu {} (OS #{})", equipamento_label(os), os.numero);
    let previsao = os
        .ficha
        .previsao_entrega
        .map(|d| format!(" Previsão de entrega: {}.", d.formatar()))
        .unwrap_or_default();
    let total = os.valor_total.formatar_com_simbolo();
    let corpo = match os.estado {
        EstadoOs::Aberta | EstadoOs::EmDiagnostico => {
            format!("Recebemos {aparelho} e já vamos avaliar.{previsao}")
        }
        EstadoOs::AguardandoAprovacao => {
            format!("O orçamento de {aparelho} ficou em {total}. Podemos seguir com o reparo?")
        }
        EstadoOs::Aprovada | EstadoOs::EmExecucao => {
            format!("Orçamento aprovado, {aparelho} está em reparo.{previsao}")
        }
        EstadoOs::Concluida => {
            format!(
                "{} está pronto para retirada. Total: {total}.",
                capitalizar(&aparelho)
            )
        }
        EstadoOs::Faturada => format!(
            "Obrigado pela preferência! {} tem garantia de {} dias.",
            capitalizar(&aparelho),
            os.garantia_dias
        ),
        EstadoOs::Reprovada | EstadoOs::Cancelada => {
            format!("{} está disponível para retirada.", capitalizar(&aparelho))
        }
    };
    let assinatura = estado
        .identidade
        .as_ref()
        .map(|i| i.nome_fantasia.trim())
        .filter(|n| !n.is_empty())
        .map(|n| format!("\n— {n}"))
        .unwrap_or_default();
    format!("{saudacao} {corpo}{assinatura}")
}

fn capitalizar(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|p| p.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// "(31) 98888-7777" a partir do que estiver gravado; devolve como veio se não reconhecer.
pub(super) fn formatar_telefone(telefone: &str) -> String {
    let d = cardeal_kernel::texto::somente_digitos(telefone);
    let d = d.strip_prefix("55").filter(|r| r.len() >= 10).unwrap_or(&d);
    match d.len() {
        11 => format!("({}) {}-{}", &d[..2], &d[2..7], &d[7..]),
        10 => format!("({}) {}-{}", &d[..2], &d[2..6], &d[6..]),
        _ => telefone.to_owned(),
    }
}

/// O link `wa.me` para o telefone (com DDI 55 quando vier só DDD + número) já com o texto.
/// `None` quando o telefone não tem dígitos suficientes para ser um celular.
pub(super) fn link_whatsapp(telefone: &str, texto: &str) -> Option<String> {
    let digitos = cardeal_kernel::texto::somente_digitos(telefone);
    let numero = match digitos.len() {
        10 | 11 => format!("55{digitos}"),
        12 | 13 if digitos.starts_with("55") => digitos,
        _ => return None,
    };
    Some(format!(
        "https://wa.me/{numero}?text={}",
        codificar_url(texto)
    ))
}

/// Codificação de URL (RFC 3986): mantém letras, dígitos e `-._~`; o resto vira `%XX` por
/// byte UTF-8.
fn codificar_url(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len() * 3);
    for b in texto.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            saida.push(char::from(b));
        } else {
            saida.push_str(&format!("%{b:02X}"));
        }
    }
    saida
}

#[cfg(test)]
mod testes_puros {
    use super::*;

    #[test]
    fn link_whatsapp_poe_ddi_e_codifica_o_texto() {
        assert_eq!(
            link_whatsapp("(31) 99999-8888", "Olá, Zé!").as_deref(),
            Some("https://wa.me/5531999998888?text=Ol%C3%A1%2C%20Z%C3%A9%21")
        );
        assert_eq!(
            link_whatsapp("+55 31 99999-8888", "x").as_deref(),
            Some("https://wa.me/5531999998888?text=x")
        );
        assert!(link_whatsapp("1234", "x").is_none());
    }

    #[test]
    fn telefone_formatado_com_ddd() {
        assert_eq!(formatar_telefone("31988887777"), "(31) 98888-7777");
        assert_eq!(formatar_telefone("+55 31 3333-4444"), "(31) 3333-4444");
        assert_eq!(formatar_telefone("ramal 12"), "ramal 12");
    }

    #[test]
    fn garantia_so_vale_para_faturada_dentro_do_prazo() {
        let hoje = Data::de_ymd(2026, 9, 25).unwrap();
        let mut os = mod_os::OrdemServico::abrir(
            Id::novo(),
            1,
            Id::novo(),
            "Notebook",
            "Tela",
            Id::novo(),
            hoje.mais_dias(-30),
            90,
        )
        .unwrap();
        assert!(!em_garantia(&os, hoje));
        os.estado = EstadoOs::Faturada;
        assert!(em_garantia(&os, hoje));
        assert!(!em_garantia(&os, hoje.mais_dias(61)));
    }
}
