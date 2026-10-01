//! "Quem é?" compartilhado entre abrir OS e lançar no Financeiro: escolher uma pessoa do
//! catálogo (busca por nome, telefone ou documento) ou cadastrar uma nova ali mesmo — nome
//! obrigatório, documento e telefone opcionais, com o aviso "Maria já está cadastrada com
//! esse telefone" que evita duplicado no balcão.

use cardeal_kernel::Id;
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::molecules::{Abas, Campo, Mascara, OpcaoBusca, SeletorBusca};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_clientes::{
    ContatoInicial, CriarPessoa, EnderecoInicial, ItemPessoa, Papel, TipoContato, TipoDocumento,
    TipoPessoa,
};

/// O estado do bloco — vive no diálogo que o usa.
#[derive(Debug, Clone, Default)]
pub struct EstadoPessoa {
    /// `true` = cadastrar agora; `false` = escolher do catálogo.
    pub novo: bool,
    /// Lançamento sem ninguém relacionado (só vale quando o bloco é `opcional`): o bloco
    /// some e quem usa o estado trata como "sem pessoa", mesmo com algo escolhido antes.
    pub nenhuma: bool,
    /// A pessoa escolhida do catálogo.
    pub selecionada: Option<Id>,
    /// O texto digitado na busca.
    pub busca: String,
    /// Nome civil ou razão social (o único obrigatório).
    pub nome: String,
    /// CPF ou CNPJ, opcional — 14 dígitos vira pessoa jurídica.
    pub documento: String,
    /// Telefone/WhatsApp, opcional.
    pub telefone: String,
    /// E-mail, opcional (só aparece quando o diálogo pede, ver [`EstadoPessoa::mostrar`]).
    pub email: String,
}

impl EstadoPessoa {
    /// Já no modo "novo" — para um diálogo que só cadastra.
    pub fn cadastro() -> Self {
        Self {
            novo: true,
            ..Self::default()
        }
    }

    /// Os dígitos do documento.
    fn digitos_documento(&self) -> String {
        cardeal_kernel::texto::somente_digitos(&self.documento)
    }

    /// Um CNPJ foi digitado (a pessoa é jurídica, o endereço é comercial).
    pub fn e_juridica(&self) -> bool {
        self.digitos_documento().len() == 14
    }

    /// Começa em "sem ninguém" — para o lançamento rápido do financeiro, onde a pessoa é a
    /// exceção (conta de luz, aluguel, venda avulsa).
    pub fn sem_pessoa() -> Self {
        Self {
            nenhuma: true,
            ..Self::default()
        }
    }

    /// A pessoa escolhida, respeitando "sem ninguém".
    pub fn escolhida(&self) -> Option<Id> {
        if self.nenhuma {
            None
        } else {
            self.selecionada
        }
    }

    /// Vai cadastrar alguém agora (e não está em "sem ninguém").
    pub fn cadastrando(&self) -> bool {
        self.novo && !self.nenhuma
    }

    /// Desenha o bloco. `papel` dá o rótulo ("Cliente"/"Favorecido"); `opcional` diz se dá
    /// para seguir sem ninguém; `com_email` mostra o campo de e-mail no cadastro. Devolve
    /// `true` quando está no modo "novo" (o chamador pode acrescentar campos, como endereço).
    pub fn mostrar(
        &mut self,
        ui: &mut egui::Ui,
        catalogo: &[ItemPessoa],
        papel: Papel,
        opcional: bool,
        com_email: bool,
    ) -> bool {
        // A pagar não é só fornecedor: sócio (pró-labore), locador, concessionária…
        let nome_papel = if papel == Papel::Fornecedor {
            "Favorecido"
        } else {
            "Cliente"
        };
        #[derive(Clone, Copy, PartialEq)]
        enum Modo {
            Nenhuma,
            Existente,
            Novo,
        }
        let sem = format!("Sem {}", nome_papel.to_lowercase());
        let existente = format!("{nome_papel} existente");
        let novo = format!("Novo {}", nome_papel.to_lowercase());
        let mut itens = Vec::with_capacity(3);
        if opcional {
            itens.push((Modo::Nenhuma, sem.as_str()));
        }
        itens.push((Modo::Existente, existente.as_str()));
        itens.push((Modo::Novo, novo.as_str()));
        let atual = if opcional && self.nenhuma {
            Modo::Nenhuma
        } else if self.novo {
            Modo::Novo
        } else {
            Modo::Existente
        };
        if let Some(m) = Abas::nova(&itens)
            .selecionada(atual)
            .id_salt("pessoa-modo")
            .mostrar(ui)
        {
            self.nenhuma = m == Modo::Nenhuma;
            self.novo = m == Modo::Novo;
        }
        if opcional && self.nenhuma {
            return false;
        }
        ui.add_space(Espaco::E8);

        if !self.novo {
            SeletorBusca::novo(nome_papel, &mut self.busca, &mut self.selecionada)
                .opcoes(opcoes(catalogo))
                .marcador("Buscar por nome, telefone ou documento…")
                .mostrar(ui);
            return false;
        }
        self.mostrar_cadastro(ui, catalogo, papel, com_email);
        true
    }

    /// Só os campos do cadastro (nome, documento, telefone, e-mail) com o aviso de duplicado —
    /// para um diálogo que só cadastra. "Usar Fulano" no aviso volta `novo` para `false` com
    /// Fulano em `selecionada`.
    pub fn mostrar_cadastro(
        &mut self,
        ui: &mut egui::Ui,
        catalogo: &[ItemPessoa],
        papel: Papel,
        com_email: bool,
    ) {
        let nome_papel = if papel == Papel::Fornecedor {
            "favorecido"
        } else {
            "cliente"
        };
        let documento = |ui: &mut egui::Ui, doc: &mut String| {
            ui.add(
                Campo::novo("Documento (opcional)", doc)
                    .mascara(Mascara::Documento)
                    .marcador("CPF ou CNPJ"),
            );
        };
        if com_email {
            ui.columns(2, |c| {
                c[0].add(Campo::novo(format!("Nome do {nome_papel}"), &mut self.nome));
                documento(&mut c[1], &mut self.documento);
            });
            ui.add_space(Espaco::E8);
            ui.columns(2, |c| {
                c[0].add(Campo::novo(
                    "Telefone/WhatsApp (opcional)",
                    &mut self.telefone,
                ));
                c[1].add(Campo::novo("E-mail (opcional)", &mut self.email));
            });
        } else {
            ui.columns(3, |c| {
                c[0].add(Campo::novo(format!("Nome do {nome_papel}"), &mut self.nome));
                c[1].add(Campo::novo(
                    "Telefone/WhatsApp (opcional)",
                    &mut self.telefone,
                ));
                documento(&mut c[2], &mut self.documento);
            });
        }
        if let Some(existente) = crate::telefone::quem_tem(catalogo, &self.telefone) {
            let (id, nome) = (existente.pessoa, existente.nome.clone());
            ui.add_space(Espaco::E4);
            ui.horizontal(|ui| {
                ui.add(
                    Rotulo::interface(format!("{nome} já está cadastrado com esse telefone."))
                        .cor(ui.cores().atencao),
                );
                if ui
                    .add(Botao::secundario(format!("Usar {nome}")).pequeno())
                    .clicked()
                {
                    self.novo = false;
                    self.selecionada = Some(id);
                }
            });
        }
    }

    /// O telefone como contato principal (WhatsApp) e o e-mail como extra — ou o e-mail
    /// como principal quando não há telefone.
    pub fn contatos(&self) -> (Option<ContatoInicial>, Vec<ContatoInicial>) {
        let contato = |tipo, valor: &str| ContatoInicial {
            tipo,
            valor: valor.to_owned(),
        };
        match (self.telefone.trim(), self.email.trim()) {
            ("", "") => (None, Vec::new()),
            ("", e) => (Some(contato(TipoContato::Email, e)), Vec::new()),
            (t, "") => (Some(contato(TipoContato::Whatsapp, t)), Vec::new()),
            (t, e) => (
                Some(contato(TipoContato::Whatsapp, t)),
                vec![contato(TipoContato::Email, e)],
            ),
        }
    }

    /// O cadastro a mandar para `clientes.criar_pessoa` (sem o e-mail extra: esse vai por
    /// [`EstadoPessoa::contatos`] para quem aceita contatos adicionais).
    ///
    /// # Errors
    /// Aviso pronto para o usuário quando falta o nome.
    pub fn para_criar(
        &self,
        papel: Papel,
        endereco: Option<EnderecoInicial>,
    ) -> Result<CriarPessoa, &'static str> {
        let nome = self.nome.trim();
        if nome.is_empty() {
            return Err(if papel == Papel::Fornecedor {
                "Informe o nome do favorecido."
            } else {
                "Informe o nome do cliente."
            });
        }
        let digitos = self.digitos_documento();
        let juridica = self.e_juridica();
        Ok(CriarPessoa {
            tipo: if juridica {
                TipoPessoa::Juridica
            } else {
                TipoPessoa::Fisica
            },
            nome: nome.to_owned(),
            nome_fantasia: None,
            papel_inicial: papel,
            documento_tipo: (!digitos.is_empty()).then_some(if juridica {
                TipoDocumento::Cnpj
            } else {
                TipoDocumento::Cpf
            }),
            documento_numero: (!digitos.is_empty()).then(|| self.documento.clone()),
            data_nascimento: None,
            endereco,
            contato: self.contatos().0,
        })
    }
}

/// As opções do seletor: telefone e documento no subtítulo, que o seletor também compara
/// (inclusive só pelos dígitos) — é como o balcão pergunta ("qual seu telefone?").
pub fn opcoes(catalogo: &[ItemPessoa]) -> Vec<OpcaoBusca<Id>> {
    catalogo
        .iter()
        .map(|c| {
            let sub: Vec<String> = c
                .telefone
                .as_deref()
                .map(crate::telefone::formatar)
                .into_iter()
                .chain(c.documento.clone())
                .collect();
            let opcao = OpcaoBusca::nova(c.pessoa, c.nome.clone());
            if sub.is_empty() {
                opcao
            } else {
                opcao.subtitulo(sub.join(" · "))
            }
        })
        .collect()
}
