# 13 — Observabilidade e diagnóstico

> O cliente do Cardeal não tem equipe de TI. A observabilidade precisa servir **primeiro ao suporte
> por telefone** e só depois ao desenvolvedor.

## 1. Tracing

`tracing` com spans estruturados. Todo comando abre um span com `correlacao`, `usuario`,
`dispositivo`, `empresa`, `comando`. Toda consulta dentro dele herda o contexto.

```rust
#[tracing::instrument(
    skip(ctx, uow),
    fields(correlacao = %ctx.correlacao, empresa = %ctx.empresa, valor = tracing::field::Empty)
)]
pub fn executar(&self, ctx: &ContextoComando, uow: &mut UnidadeDeTrabalho) -> Resultado<Saida> {
    tracing::Span::current().record("valor", self.total.to_string().as_str());
    // ...
}
```

Níveis e destinos:

| Nível | Vai para | Retenção |
|---|---|---|
| `ERROR`, `WARN` | Arquivo + tela de Diagnóstico + alerta | 90 dias |
| `INFO` | Arquivo rotativo | 30 dias |
| `DEBUG` | Só com `--verboso` ou botão "Coletar diagnóstico" | sessão |
| `TRACE` | Só em desenvolvimento | — |

Rotação diária, compressão zstd, teto de 500 MB total.

**Nenhum dado pessoal em log.** O logger tem uma camada que redige CPF, CNPJ, e-mail, telefone e
nome próprio. O teste `logs_sem_pii` gera cenário com dados sintéticos reconhecíveis e falha se
algum aparecer no arquivo.

## 2. Métricas

Coletadas em memória (histogramas HDR compactos, ~200 KB) e expostas em `/metrics`
(formato Prometheus) quando habilitado.

| Métrica | Tipo | Para quê |
|---|---|---|
| `cardeal_comando_duracao_ms{comando}` | histograma | p50/p95/p99 por comando |
| `cardeal_consulta_duracao_ms{consulta}` | histograma | detectar consulta sem índice |
| `cardeal_escritor_lote_tamanho` | histograma | eficácia do group commit |
| `cardeal_escritor_fila_espera_ms` | histograma | saturação do escritor |
| `cardeal_fsync_total` | contador | custo real de durabilidade |
| `cardeal_outbox_pendente` | medidor | atraso de eventos |
| `cardeal_fiscal_fila{estado}` | medidor | documentos presos |
| `cardeal_sessoes_ativas` | medidor | quem está usando |
| `cardeal_terminais_autonomos` | medidor | **crítico**: terminais sem servidor |
| `cardeal_rss_bytes` | medidor | orçamento do Pilar I |
| `cardeal_banco_bytes` | medidor | crescimento |
| `cardeal_backup_idade_s` | medidor | último backup bem-sucedido |

## 3. A tela de Diagnóstico

Acessível por `Ctrl+K → diagnóstico`, ou pelo rodapé. É a tela que o suporte pede para o cliente abrir.

```
┌──────────────────────────────────────────────────────────────────────────┐
│  Diagnóstico do sistema                    [Coletar pacote de suporte]   │
├──────────────────────────────────────────────────────────────────────────┤
│  SAÚDE                                                                   │
│  ✅ Banco de dados            íntegro · 1,4 GB · WAL 3,2 MB              │
│  ✅ Razão                     balanceado · última prova há 6 h           │
│  ✅ Backup                    há 12 min · verificado ✓                   │
│  ✅ Servidor                  3 terminais conectados                     │
│  ⚠️  Fiscal                   2 documentos rejeitados                    │
│  ✅ Energia                   rede elétrica                              │
│                                                                          │
│  DESEMPENHO (última hora)                                                │
│  Comandos            1.847      p50 12 ms    p95  38 ms    p99  91 ms    │
│  Consultas          14.203      p50  3 ms    p95  11 ms    p99  44 ms    │
│  Lote do escritor      média 11,4 operações por fsync                    │
│  Memória            38,2 MB     pico 41,7 MB                             │
│                                                                          │
│  TERMINAIS                                                               │
│  PDV 1 · Caixa da frente     conectado    v0.9.2   latência  0,7 ms      │
│  PDV 2 · Caixa do fundo      AUTÔNOMO     v0.9.2   há 4 min · 7 na fila  │
│  Retaguarda · Escritório     conectado    v0.9.2   latência  1,1 ms      │
│                                                                          │
│  ÚLTIMOS ERROS                                                           │
│  14:22  NFC-e 4471 rejeitada · 539 duplicidade de número      [detalhe]  │
│  11:08  Conflito de edição em Cliente #1204 · resolvido       [detalhe]  │
└──────────────────────────────────────────────────────────────────────────┘
```

### 3.1 Pacote de suporte

Um botão gera um `.zip` com: logs recentes redigidos, métricas, versões, esquema do banco,
resultado da verificação de integridade, configuração **sem segredos** e as últimas 200 linhas de
auditoria. **Não inclui dados de negócio.** O conteúdo é listado na tela antes de gerar — o cliente
vê exatamente o que vai enviar.

## 4. Alertas ao gestor

Alertas aparecem no Pulso e, opcionalmente, por notificação do sistema, e-mail ou WhatsApp:

| Evento | Severidade |
|---|---|
| Backup falhou 2 vezes seguidas | Crítico |
| Razão desbalanceado | Crítico |
| Corrupção detectada | Crítico |
| Terminal autônomo há mais de 30 min | Alto |
| Documento fiscal rejeitado | Alto |
| Certificado vence em menos de 30 dias | Alto |
| Caixa aberto há mais de 12 h | Médio |
| Disco com menos de 10% livre | Médio |
| Versão nova disponível | Baixo |

## 5. Depuração em produção

- **Modo verboso temporário:** o suporte pede `Ctrl+K → verboso 10min`. Aumenta o nível de log e
  volta ao normal sozinho. Sem reiniciar, sem editar arquivo.
- **Explicar consulta:** em desenvolvimento e sob permissão de admin, qualquer grade tem
  `Ctrl+Shift+E` que mostra o SQL, o plano (`EXPLAIN QUERY PLAN`) e o tempo.
- **Reprodutor de comando:** um comando registrado pode ser reexecutado contra uma cópia do banco,
  em modo somente leitura, para reproduzir um bug com os dados reais do cliente sem tocar na base.
- **Linha do tempo da entidade:** qualquer registro tem aba "Histórico" mostrando a auditoria completa
  em linguagem natural: *"Ana alterou o limite de crédito de R$ 2.000,00 para R$ 3.500,00 em
  02/09 às 14:11, no terminal Retaguarda."*

## 6. Telemetria

**Desligada por padrão.** Se o cliente ativar, envia apenas: versão, perfil de empresa, contagem de
módulos ativos, métricas de desempenho agregadas e ocorrências de erro (sem conteúdo). Nunca dados de
negócio, nunca dados pessoais. O que é enviado é exibido na tela de configuração, item por item.
