//! O comprovante da OS em PDF.

use super::*;

/// Rótulo amigável do estado da OS para o comprovante — o cliente não deveria ler
/// "EmDiagnostico" no papel que leva pra casa.
pub(super) fn situacao_amigavel(e: EstadoOs) -> &'static str {
    match e {
        EstadoOs::Aberta => "Aberta — aguardando diagnóstico",
        EstadoOs::EmDiagnostico => "Em diagnóstico",
        EstadoOs::AguardandoAprovacao => "Aguardando aprovação do orçamento",
        EstadoOs::Aprovada => "Orçamento aprovado — aguardando execução",
        EstadoOs::Reprovada => "Orçamento reprovado",
        EstadoOs::EmExecucao => "Em execução",
        EstadoOs::Concluida => "Execução concluída — aguardando faturamento",
        EstadoOs::Faturada => "Concluída e faturada",
        EstadoOs::Cancelada => "Cancelada",
    }
}

/// Monta e salva o comprovante de OS em PDF — mesmo padrão de
/// `tela_orcamentos::gerar_pdf` (identidade visual da empresa + diálogo nativo "Salvar
/// como"). Gerável em qualquer estado da OS: uma OS recém-aberta ainda sem laudo/orçamento
/// vira o comprovante de entrada (só cliente + equipamento); uma faturada leva o laudo, os
/// itens aplicados e a garantia — o mesmo documento cobre as duas pontas do fluxo.
pub(super) fn gerar_pdf(ctx: &egui::Context, estado: &EstadoTelaOs, d: &DetalheOrdem) {
    let ident = estado.identidade.clone().unwrap_or_default();
    let empresa = IdentidadeEmpresa {
        nome_fantasia: ident.nome_fantasia,
        razao_social: ident.razao_social,
        cnpj: ident.cnpj,
        endereco: ident.endereco,
        telefone: ident.telefone,
        email: ident.email,
        site: ident.site,
        logo_png: ident.logo_png,
    };

    let os = &d.ordem;
    let cliente = estado.clientes.iter().find(|c| c.pessoa == os.cliente);
    let tecnico = estado
        .usuarios
        .iter()
        .find(|u| u.id == os.tecnico_responsavel);

    let mut itens: Vec<ItemPdf> = Vec::new();
    for peca in &d.itens_peca {
        let nome_produto = estado
            .produtos
            .iter()
            .find(|p| p.produto == peca.produto)
            .map_or_else(|| "Peça".to_owned(), |p| p.nome.clone());
        let rotulo = if peca.coberto_garantia {
            format!("Peça — {nome_produto} (garantia)")
        } else {
            format!("Peça — {nome_produto}")
        };
        itens.push(ItemPdf {
            descricao: rotulo,
            quantidade: peca.quantidade,
            unidade: "un".to_owned(),
            preco_unitario: peca.preco_unitario,
            desconto_pct: Percentual::ZERO,
            total: peca.total_cobrado(),
        });
    }
    for mdo in &d.itens_mao_de_obra {
        let quantidade = mdo.horas.unwrap_or_else(|| Quantidade::unidades(1));
        itens.push(ItemPdf {
            descricao: format!("Mão de obra — {}", mdo.descricao),
            quantidade,
            unidade: if mdo.horas.is_some() {
                "h".to_owned()
            } else {
                "srv".to_owned()
            },
            preco_unitario: Preco::centavos(mdo.valor.em_centavos()),
            desconto_pct: Percentual::ZERO,
            total: mdo.valor,
        });
    }
    let subtotal = itens.iter().fold(Dinheiro::ZERO, |acc, i| acc + i.total);

    let doc = ComprovanteOsPdf {
        numero: os.numero,
        situacao: situacao_amigavel(os.estado).to_owned(),
        cliente_nome: cliente.map_or_else(String::new, |c| c.nome.clone()),
        cliente_documento: cliente
            .and_then(|c| c.documento.clone())
            .unwrap_or_default(),
        cliente_contato: String::new(),
        equipamento: os.equipamento.clone(),
        data_abertura: os.data_abertura,
        // O defeito relatado agora é capturado na abertura (`OrdemServico::defeito_relatado`,
        // obrigatório) — o laudo só entra como respaldo para uma OS aberta antes desta versão
        // (migração aditiva, coluna nova preenchida com "" nas linhas antigas).
        defeito_relatado: if os.defeito_relatado.trim().is_empty() {
            d.laudo
                .as_ref()
                .map_or_else(String::new, |l| l.descricao_problema.clone())
        } else {
            os.defeito_relatado.clone()
        },
        diagnostico: d
            .laudo
            .as_ref()
            .and_then(|l| l.diagnostico.clone())
            .unwrap_or_default(),
        itens,
        subtotal,
        total: subtotal,
        garantia_dias: os.garantia_dias,
        aprovado_por: os.aprovado_por.clone().unwrap_or_default(),
        tecnico_responsavel: tecnico.map_or_else(String::new, |t| t.nome.clone()),
    };

    let bytes = match gerar_comprovante_os(&empresa, &doc, Instante::agora()) {
        Ok(b) => b,
        Err(e) => {
            notificar(ctx, Notificacao::erro(format!("PDF: {e}")));
            return;
        }
    };
    let nome = format!("OS-{:04}.pdf", os.numero);
    let Some(caminho) = rfd::FileDialog::new()
        .set_file_name(&nome)
        .add_filter("PDF", &["pdf"])
        .save_file()
    else {
        return;
    };
    match std::fs::write(&caminho, &bytes) {
        Ok(()) => {
            let _ = open::that_detached(&caminho);
            notificar(
                ctx,
                Notificacao::sucesso(format!("PDF salvo em {}", caminho.display())),
            );
        }
        Err(e) => notificar(
            ctx,
            Notificacao::erro(format!("Não foi possível salvar: {e}")),
        ),
    }
}
