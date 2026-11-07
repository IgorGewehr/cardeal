//! Os documentos da OS em PDF: a montagem dos dados e o salvar/abrir.

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

/// A identidade visual da empresa para os documentos.
pub(super) fn identidade_empresa(estado: &EstadoTelaOs) -> IdentidadeEmpresa {
    let ident = estado.identidade.clone().unwrap_or_default();
    IdentidadeEmpresa {
        nome_fantasia: ident.nome_fantasia,
        razao_social: ident.razao_social,
        cnpj: ident.cnpj,
        endereco: ident.endereco,
        telefone: ident.telefone,
        email: ident.email,
        site: ident.site,
        logo_png: ident.logo_png,
    }
}

/// Os dados da OS no formato do PDF — o mesmo para comprovante, orçamento e relatório.
/// Gerável em qualquer estado: recém-aberta (só cliente + equipamento) até faturada (laudo,
/// itens aplicados e garantia).
pub(super) fn montar_documento(estado: &EstadoTelaOs, d: &DetalheOrdem) -> ComprovanteOsPdf {
    let os = &d.ordem;
    let cliente = estado.clientes.iter().find(|c| c.pessoa == os.cliente);
    let tecnico = estado
        .usuarios
        .iter()
        .find(|u| u.id == os.tecnico_responsavel);

    let mut itens: Vec<ItemPdf> = Vec::new();
    for peca in &d.itens_peca {
        let nome_produto = nome_do_produto(&estado.produtos, peca.produto);
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
            descricao: format!("Serviço — {}", mdo.descricao),
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

    ComprovanteOsPdf {
        numero: os.numero,
        situacao: situacao_amigavel(os.estado).to_owned(),
        cliente_nome: cliente.map_or_else(String::new, |c| c.nome.clone()),
        cliente_documento: cliente
            .and_then(|c| c.documento.clone())
            .unwrap_or_default(),
        cliente_contato: cliente
            .and_then(|c| c.telefone.as_deref())
            .map(crate::telefone::formatar)
            .unwrap_or_default(),
        equipamento: os.equipamento.clone(),
        data_abertura: os.data_abertura,
        // O defeito relatado é capturado na abertura; o laudo só entra como respaldo para
        // uma OS aberta antes disso (coluna preenchida com "" nas linhas antigas).
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
    }
}

/// Pergunta onde salvar, grava e abre o PDF. Devolve `true` se salvou.
pub(super) fn salvar_e_abrir_pdf(ctx: &egui::Context, bytes: &[u8], nome: &str) -> bool {
    let Some(caminho) = rfd::FileDialog::new()
        .set_file_name(nome)
        .add_filter("PDF", &["pdf"])
        .save_file()
    else {
        return false;
    };
    match std::fs::write(&caminho, bytes) {
        Ok(()) => {
            let _ = open::that_detached(&caminho);
            notificar(
                ctx,
                Notificacao::sucesso(format!("PDF salvo em {}", caminho.display())),
            );
            true
        }
        Err(e) => {
            notificar(
                ctx,
                Notificacao::erro(format!("Não foi possível salvar: {e}")),
            );
            false
        }
    }
}
