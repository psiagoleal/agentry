// Caminho relativo: crates/core/src/provider/claude_cli.rs
//! Provider `claude-cli` (ADR-0040): acesso à assinatura **Claude Pro/Max**
//! via *spawn* do binário `claude` em modo *headless*
//! (`claude -p --output-format stream-json`).
//!
//! **Por que um subprocesso e não a Messages API.** O token OAuth do Claude
//! Code é emitido *para* o Claude Code; reusá-lo a partir de um CLI de
//! terceiros viola os termos de uso. Este provider nunca lê, copia ou
//! persiste esse token — a autenticação é inteiramente do binário `claude`,
//! que resolve sozinho o login que o usuário já fez.
//!
//! **Auditoria equivalente (condição da ADR-0040, não um extra).** Como o
//! egresso não passa pelo [`crate::transport::Transport`], ele fica fora da
//! `Allowlist` e do audit log de egresso — os dois mecanismos que sustentam o
//! objetivo de conformidade do projeto. Este módulo compensa emitindo ele
//! próprio uma [`AuditEntry`] por invocação, no **mesmo** [`AuditSink`] já
//! fiado pela CLI (portanto no mesmo `.agentry/audit.log` da ADR-0037). A
//! classe de egresso é verificada **antes** do *spawn*: sessão que não permite
//! nuvem produz `AuditEntry` com [`AuditOutcome::Blocked`] e erro, sem nunca
//! criar o processo — espelhando o `Transport` sob `Allowlist`, que bloqueia
//! antes de tocar a rede.
//!
//! ## Limitação estrutural: sem *tool-calling*
//!
//! `claude -p` **não é um endpoint de modelo — é um agente completo**, com
//! laço próprio, ferramentas próprias (`Read`/`Edit`/`Bash`/…) e sistema de
//! permissão próprio. Isso é incompatível com o contrato de [`LlmProvider`],
//! onde quem executa tool é o `agentry` (sob `PermissionGate`, *checkpoints* e
//! guardrails).
//!
//! A escolha desta implementação é **desligar as ferramentas do Claude Code**
//! (`--tools ""`), usando o subprocesso como gerador de **texto puro**. As
//! consequências, deliberadas:
//!
//! - o modelo nunca executa nada na máquina por fora do `agentry` — nenhuma
//!   edição de arquivo escapa do `PermissionGate`/`CheckpointStore`;
//! - em compensação, este provider **não faz tool-calling**. Serve para
//!   conversa, `/compact`, revisão (`guardrail-compliance`) e qualquer
//!   task-class de texto — **não** para o laço agêntico principal.
//!
//! Tool-calling real por esta via exigiria expor as tools do `agentry` como
//! **servidor** MCP para o `claude -p` consumir (`--mcp-config
//! --strict-mcp-config`) — decisão de arquitetura própria, registrada como
//! trabalho futuro na ADR-0040, não assumida aqui.
//!
//! Quando uma [`ChatRequest`] chega com `tools` não vazias, o provider avisa
//! em `stderr` **uma vez por processo** em vez de ignorar em silêncio: o
//! silêncio nesse exato ponto foi a causa-raiz do bug de *tool-calling* do
//! Ollama (emenda de 2026-07-24 à ADR-0012), e não se repete aqui.

use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Deserialize;

use crate::config::privacy::EgressClass;
use crate::egress::audit::{AuditEntry, AuditOutcome};
use crate::model::{ContentBlock, Message, Role, StreamEvent, Usage};
use crate::provider::{
    BoxFuture, ChatRequest, ChatResponse, ChatStream, EmbeddingsRequest, EmbeddingsResponse,
    LlmProvider, ProviderError, ToolSpec,
};
use crate::transport::AuditSink;

/// Nome do binário invocado. Sem caminho absoluto de propósito: resolve pelo
/// `PATH`, como qualquer outra invocação que o usuário faria à mão.
pub const CLAUDE_BINARY: &str = "claude";

/// Destino registrado na auditoria. Não é um host — é justamente o ponto: a
/// entrada precisa deixar claro que o egresso saiu por um **subprocesso**, não
/// pelo `Transport`, para quem audita não confundir com uma chamada HTTP
/// coberta pela `Allowlist`.
const AUDIT_DESTINATION: &str = "subprocess:claude-cli (api.anthropic.com)";

/// Provider que delega a geração ao binário `claude` autenticado com a
/// assinatura Pro/Max do usuário (ADR-0040).
pub struct ClaudeCliProvider {
    audit_sink: Arc<dyn AuditSink>,
    /// Classe de egresso **da sessão** — verificada antes de todo *spawn*.
    egress_class: EgressClass,
    profile: Option<String>,
    /// Binário a invocar; parametrizado para os testes poderem apontar para um
    /// script controlado, sem depender de um `claude` real instalado.
    binary: String,
    /// Garante que o aviso de "tools ignoradas" saia uma vez por processo, não
    /// a cada turno (viraria ruído e o usuário pararia de ler).
    ja_avisou_sobre_tools: AtomicBool,
}

impl ClaudeCliProvider {
    /// Cria o provider para uma sessão com `egress_class` resolvida.
    #[must_use]
    pub fn new(
        audit_sink: Arc<dyn AuditSink>,
        egress_class: EgressClass,
        profile: Option<String>,
    ) -> Self {
        Self {
            audit_sink,
            egress_class,
            profile,
            binary: CLAUDE_BINARY.to_string(),
            ja_avisou_sobre_tools: AtomicBool::new(false),
        }
    }

    /// Substitui o binário invocado — usado pelos testes para apontar a um
    /// script que imita o `stream-json` sem exigir um `claude` de verdade.
    #[must_use]
    pub fn with_binary(mut self, binary: impl Into<String>) -> Self {
        self.binary = binary.into();
        self
    }

    /// Verifica a classe de egresso e audita a decisão — **sempre** emite
    /// exatamente uma [`AuditEntry`], permitida ou bloqueada (diretriz de
    /// conformidade da ADR-0040).
    ///
    /// # Errors
    ///
    /// Devolve [`ProviderError::Network`] quando a classe ativa da sessão não
    /// alcança a nuvem — sem nunca criar o processo.
    fn autorizar_e_auditar(&self, task: &str) -> Result<(), ProviderError> {
        if self.egress_class.permits(EgressClass::CloudOk) {
            self.audit_sink.record(
                AuditEntry::allowed(
                    AUDIT_DESTINATION,
                    self.profile.clone(),
                    self.egress_class,
                    task,
                )
                // Este destino é sempre a nuvem pública: declará-lo
                // explicitamente é o que faz a linha do audit log dizer "saiu
                // da máquina", em vez de repetir o teto da sessão.
                .with_destination_class(EgressClass::CloudOk),
            );
            return Ok(());
        }

        let motivo = format!(
            "classe de egresso da sessão ({:?}) não permite alcançar a nuvem da Anthropic",
            self.egress_class
        );
        self.audit_sink.record(AuditEntry::new(
            AUDIT_DESTINATION,
            self.profile.clone(),
            self.egress_class,
            task,
            AuditOutcome::Blocked,
            Some(motivo.clone()),
        ));
        Err(ProviderError::Network(format!(
            "egresso bloqueado: {motivo}"
        )))
    }

    /// Avisa (uma vez por processo) que as `tools` da requisição não serão
    /// usadas — ver a limitação estrutural na doc do módulo.
    fn avisar_sobre_tools_ignoradas(&self, tools: &[ToolSpec]) {
        if tools.is_empty() || self.ja_avisou_sobre_tools.swap(true, Ordering::SeqCst) {
            return;
        }
        eprintln!(
            "[claude-cli] aviso: {} tool(s) foram declaradas, mas este provider não faz \
             tool-calling (ADR-0040) — o modelo responderá só com texto. Para o laço \
             agêntico completo, use o provider `ollama`, `litellm` ou `anthropic`.",
            tools.len()
        );
    }

    /// Monta o comando `claude` já com as flags fixas da ADR-0040.
    ///
    /// `--tools ""` desliga as ferramentas embutidas do Claude Code: nenhuma
    /// ação na máquina do usuário escapa do `PermissionGate` do `agentry`.
    fn montar_comando(&self, model: &str) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(&self.binary);
        cmd.arg("-p")
            .arg("--output-format")
            .arg("stream-json")
            // `--verbose` é exigido pelo próprio CLI junto de
            // `stream-json`; sem ele a invocação falha.
            .arg("--verbose")
            .arg("--model")
            .arg(model)
            .arg("--tools")
            .arg("")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }
}

/// Achata o histórico num único *prompt* de texto.
///
/// `claude -p` recebe **um** prompt, não uma lista de mensagens com papéis —
/// então o histórico é serializado com rótulos legíveis. Mensagens de sistema
/// vêm primeiro (é o que mais se aproxima de um *system prompt*); blocos de
/// tool não têm representação e são omitidos, coerente com este provider não
/// fazer tool-calling.
fn achatar_historico(messages: &[Message]) -> String {
    let mut sistema = Vec::new();
    let mut conversa = Vec::new();

    for message in messages {
        let texto: String = message
            .content
            .iter()
            .filter_map(|bloco| match bloco {
                ContentBlock::Text { text } => Some(text.as_str()),
                ContentBlock::ToolCall(_) | ContentBlock::ToolResult(_) => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        if texto.trim().is_empty() {
            continue;
        }

        match message.role {
            Role::System => sistema.push(texto),
            Role::User => conversa.push(format!("Usuário: {texto}")),
            Role::Assistant => conversa.push(format!("Assistente: {texto}")),
            Role::Tool => {}
        }
    }

    let mut partes = sistema;
    partes.extend(conversa);
    partes.join("\n\n")
}

// ---- Formato de fio do `stream-json` (interno; superfície de terceiro) ----

/// Uma linha do `stream-json`. Só o que este provider consome é modelado —
/// `#[serde(other)]` absorve `system`/`rate_limit_event`/etc. sem quebrar
/// quando o Claude Code adicionar tipos novos (superfície de terceiro, muda
/// entre versões: a ADR-0040 aceita esse acoplamento explicitamente).
#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum LinhaStreamJson {
    Assistant {
        message: MensagemAssistente,
    },
    Result(Box<ResultadoFinal>),
    #[serde(other)]
    Outro,
}

#[derive(Deserialize, Debug)]
struct MensagemAssistente {
    #[serde(default)]
    content: Vec<BlocoDeConteudo>,
}

/// Só `text` interessa: `thinking` é descartado (o `StreamEvent` do MT-02 não
/// tem variante de raciocínio) e `tool_use` não ocorre com `--tools ""`.
#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BlocoDeConteudo {
    Text {
        text: String,
    },
    #[serde(other)]
    Outro,
}

#[derive(Deserialize, Debug, Default)]
struct ResultadoFinal {
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    usage: UsoDoResultado,
}

#[derive(Deserialize, Debug, Default, Clone, Copy)]
struct UsoDoResultado {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

impl From<UsoDoResultado> for Usage {
    fn from(u: UsoDoResultado) -> Self {
        Self {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
        }
    }
}

/// Texto e uso extraídos de uma execução completa do `stream-json`.
#[derive(Default)]
struct SaidaAcumulada {
    texto: String,
    usage: Usage,
}

/// Interpreta uma linha do `stream-json`, acumulando texto/uso.
///
/// Linha em branco ou JSON não reconhecido é **ignorada**, nunca fatal: o
/// formato é superfície de terceiro e pode ganhar campos entre versões do
/// Claude Code. O que **é** fatal (`Err`) é o próprio Claude Code reportar
/// falha (`is_error`), que não pode ser mascarada.
fn aplicar_linha(linha: &str, acc: &mut SaidaAcumulada) -> Result<(), ProviderError> {
    let linha = linha.trim();
    if linha.is_empty() {
        return Ok(());
    }
    let Ok(parsed) = serde_json::from_str::<LinhaStreamJson>(linha) else {
        return Ok(());
    };

    match parsed {
        LinhaStreamJson::Assistant { message } => {
            for bloco in message.content {
                if let BlocoDeConteudo::Text { text } = bloco {
                    acc.texto.push_str(&text);
                }
            }
        }
        LinhaStreamJson::Result(resultado) => {
            if resultado.is_error {
                return Err(ProviderError::InvalidResponse(format!(
                    "claude CLI reportou erro: {}",
                    resultado.result.as_deref().unwrap_or("(sem detalhe)")
                )));
            }
            acc.usage = resultado.usage.into();
            // O campo `result` é a resposta final canônica; as linhas
            // `assistant` podem trazer o mesmo texto em pedaços. Preferir
            // `result` evita duplicação quando os dois estão presentes.
            if let Some(texto) = resultado.result {
                if !texto.is_empty() {
                    acc.texto = texto;
                }
            }
        }
        LinhaStreamJson::Outro => {}
    }
    Ok(())
}

impl LlmProvider for ClaudeCliProvider {
    fn name(&self) -> &str {
        "claude-cli"
    }

    fn chat(&self, request: ChatRequest) -> BoxFuture<'_, Result<ChatResponse, ProviderError>> {
        Box::pin(async move {
            self.autorizar_e_auditar("chat")?;
            self.avisar_sobre_tools_ignoradas(&request.tools);

            let prompt = achatar_historico(&request.messages);
            let mut filho = self
                .montar_comando(&request.model)
                .spawn()
                .map_err(|e| erro_de_spawn(&self.binary, &e))?;

            escrever_prompt(&mut filho, &prompt).await?;

            let saida = filho
                .wait_with_output()
                .await
                .map_err(|e| ProviderError::Network(format!("falha ao aguardar o claude: {e}")))?;

            if !saida.status.success() {
                return Err(ProviderError::Network(format!(
                    "claude CLI terminou com status {}: {}",
                    saida.status,
                    String::from_utf8_lossy(&saida.stderr).trim()
                )));
            }

            let mut acc = SaidaAcumulada::default();
            for linha in String::from_utf8_lossy(&saida.stdout).lines() {
                aplicar_linha(linha, &mut acc)?;
            }

            Ok(ChatResponse {
                message: Message::assistant(acc.texto),
                usage: acc.usage,
            })
        })
    }

    fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> BoxFuture<'_, Result<ChatStream, ProviderError>> {
        Box::pin(async move {
            self.autorizar_e_auditar("chat_stream")?;
            self.avisar_sobre_tools_ignoradas(&request.tools);

            let prompt = achatar_historico(&request.messages);
            let mut filho = self
                .montar_comando(&request.model)
                .spawn()
                .map_err(|e| erro_de_spawn(&self.binary, &e))?;

            escrever_prompt(&mut filho, &prompt).await?;

            let stdout = filho
                .stdout
                .take()
                .ok_or_else(|| ProviderError::Network("stdout do claude indisponível".into()))?;

            let (tx, rx) = tokio::sync::mpsc::channel(16);
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};

                if tx.send(Ok(StreamEvent::MessageStart)).await.is_err() {
                    return;
                }

                let mut linhas = BufReader::new(stdout).lines();
                let mut acc = SaidaAcumulada::default();
                // Quanto do texto já foi emitido como delta: `aplicar_linha`
                // pode *substituir* o acumulado quando o `result` final chega,
                // então o delta é sempre o sufixo ainda não enviado.
                let mut emitido = 0usize;

                loop {
                    match linhas.next_line().await {
                        Ok(Some(linha)) => {
                            if let Err(erro) = aplicar_linha(&linha, &mut acc) {
                                let _ = tx.send(Err(erro)).await;
                                return;
                            }
                            if acc.texto.len() > emitido {
                                let delta = acc.texto[emitido..].to_string();
                                emitido = acc.texto.len();
                                if tx
                                    .send(Ok(StreamEvent::TextDelta { text: delta }))
                                    .await
                                    .is_err()
                                {
                                    return;
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            let _ = tx
                                .send(Err(ProviderError::Network(format!(
                                    "falha ao ler a saída do claude: {e}"
                                ))))
                                .await;
                            return;
                        }
                    }
                }

                let _ = filho.wait().await;
                let _ = tx
                    .send(Ok(StreamEvent::MessageEnd { usage: acc.usage }))
                    .await;
            });

            Ok(rx)
        })
    }

    fn embeddings(
        &self,
        _request: EmbeddingsRequest,
    ) -> BoxFuture<'_, Result<EmbeddingsResponse, ProviderError>> {
        Box::pin(async move {
            Err(ProviderError::Unsupported(
                "ClaudeCliProvider não implementa embeddings — use o Ollama local (ADR-0039)"
                    .into(),
            ))
        })
    }
}

/// Erro de *spawn* com a causa provável explícita — binário ausente é o modo
/// de falha mais comum deste provider (ADR-0040) e merece mensagem acionável,
/// nunca um `io::Error` cru.
fn erro_de_spawn(binario: &str, e: &std::io::Error) -> ProviderError {
    if e.kind() == std::io::ErrorKind::NotFound {
        return ProviderError::Network(format!(
            "binário '{binario}' não encontrado no PATH — o provider `claude-cli` exige o \
             Claude Code instalado e autenticado (`claude login`)"
        ));
    }
    ProviderError::Network(format!("falha ao executar '{binario}': {e}"))
}

/// Escreve o prompt no *stdin* do filho e o fecha — `claude -p` só começa a
/// processar quando vê EOF.
async fn escrever_prompt(
    filho: &mut tokio::process::Child,
    prompt: &str,
) -> Result<(), ProviderError> {
    use tokio::io::AsyncWriteExt;

    let mut stdin = filho
        .stdin
        .take()
        .ok_or_else(|| ProviderError::Network("stdin do claude indisponível".into()))?;
    stdin
        .write_all(prompt.as_bytes())
        .await
        .map_err(|e| ProviderError::Network(format!("falha ao enviar o prompt: {e}")))?;
    stdin
        .shutdown()
        .await
        .map_err(|e| ProviderError::Network(format!("falha ao fechar o stdin: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Sink que guarda as entradas para os testes de auditoria — o ponto
    /// central da ADR-0040 é justamente que este provider audite sozinho.
    #[derive(Default)]
    struct SinkColetor {
        entradas: Mutex<Vec<AuditEntry>>,
    }
    impl AuditSink for SinkColetor {
        fn record(&self, entry: AuditEntry) {
            self.entradas.lock().expect("mutex sadio").push(entry);
        }
    }

    /// Diretório temporário auto-removido — mesma convenção de
    /// `cli/src/audit_sink.rs` (o projeto não adota `tempfile`, ADR-0004:
    /// nenhuma dependência nova sem necessidade real).
    struct TempDir(std::path::PathBuf);
    impl TempDir {
        fn new() -> Self {
            let caminho = std::env::temp_dir().join(format!(
                "agentry-claude-cli-teste-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("relógio não deve estar antes de 1970")
                    .as_nanos()
            ));
            std::fs::create_dir_all(&caminho).expect("deve criar o diretório temporário");
            Self(caminho)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Script que imita o `stream-json` do Claude Code, para não exigir um
    /// `claude` real (nem consumir a assinatura) nos testes.
    fn script_falso(corpo: &str) -> (TempDir, String) {
        use std::io::Write as _;
        let dir = TempDir::new();
        let caminho = dir.0.join("claude-falso.sh");
        let mut f = std::fs::File::create(&caminho).expect("criar script");
        // `cat > /dev/null` consome o prompt do stdin: sem isso o script
        // terminaria antes da escrita e o provider veria um "broken pipe".
        writeln!(f, "#!/bin/sh\ncat > /dev/null\ncat <<'FIM'\n{corpo}\nFIM").expect("escrever");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&caminho, std::fs::Permissions::from_mode(0o755))
                .expect("chmod");
        }
        let s = caminho.to_string_lossy().into_owned();
        (dir, s)
    }

    fn provider_com_script(
        corpo: &str,
        classe: EgressClass,
    ) -> (TempDir, ClaudeCliProvider, Arc<SinkColetor>) {
        let (dir, caminho) = script_falso(corpo);
        let sink = Arc::new(SinkColetor::default());
        let provider = ClaudeCliProvider::new(sink.clone(), classe, Some("pessoal".into()))
            .with_binary(caminho);
        (dir, provider, sink)
    }

    const SAIDA_OK: &str = r#"{"type":"system","subtype":"init","tools":[]}
{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"hmm"}]}}
{"type":"assistant","message":{"content":[{"type":"text","text":"olá"}]}}
{"type":"rate_limit_event","rate_limit_info":{"status":"allowed"}}
{"type":"result","subtype":"success","is_error":false,"result":"olá","usage":{"input_tokens":12,"output_tokens":3}}"#;

    #[tokio::test]
    async fn chat_extrai_texto_e_uso_do_stream_json() {
        let (_dir, provider, _sink) = provider_com_script(SAIDA_OK, EgressClass::CloudOk);

        let resposta = provider
            .chat(ChatRequest::new("claude-opus-5", vec![Message::user("oi")]))
            .await
            .expect("script válido deve responder");

        assert_eq!(resposta.message, Message::assistant("olá"));
        assert_eq!(resposta.usage.input_tokens, 12);
        assert_eq!(resposta.usage.output_tokens, 3);
    }

    #[tokio::test]
    async fn chat_audita_uma_entrada_permitida_por_invocacao() {
        let (_dir, provider, sink) = provider_com_script(SAIDA_OK, EgressClass::CloudOk);

        provider
            .chat(ChatRequest::new("claude-opus-5", vec![Message::user("oi")]))
            .await
            .expect("deve responder");

        let entradas = sink.entradas.lock().expect("mutex sadio");
        assert_eq!(entradas.len(), 1, "exatamente uma entrada por invocação");
        assert_eq!(entradas[0].outcome, AuditOutcome::Allowed);
        assert!(
            entradas[0].destination.contains("subprocess"),
            "a trilha precisa deixar claro que o egresso não passou pelo Transport: {}",
            entradas[0].destination
        );
    }

    /// O ponto de conformidade central da ADR-0040: bloquear **antes** do
    /// *spawn*, como o `Transport` faz antes de tocar a rede.
    #[tokio::test]
    async fn classe_de_egresso_insuficiente_bloqueia_sem_criar_o_processo() {
        // Binário inexistente de propósito: se o provider tentasse executar,
        // o erro seria de "não encontrado", não de egresso bloqueado.
        let sink = Arc::new(SinkColetor::default());
        let provider =
            ClaudeCliProvider::new(sink.clone(), EgressClass::LocalOnly, Some("empresa".into()))
                .with_binary("/caminho/que/nao/existe/claude");

        let erro = provider
            .chat(ChatRequest::new("claude-opus-5", vec![Message::user("oi")]))
            .await
            .expect_err("sessão local-only não pode alcançar a nuvem");

        match erro {
            ProviderError::Network(msg) => assert!(
                msg.contains("egresso bloqueado"),
                "erro deve ser de egresso, não de spawn: {msg}"
            ),
            outro => panic!("esperado erro de egresso, veio {outro:?}"),
        }

        let entradas = sink.entradas.lock().expect("mutex sadio");
        assert_eq!(entradas.len(), 1, "bloqueio também audita");
        assert_eq!(entradas[0].outcome, AuditOutcome::Blocked);
        assert!(entradas[0].reason.is_some(), "bloqueio registra o motivo");
    }

    #[tokio::test]
    async fn cloud_opt_out_tambem_bloqueia() {
        let sink = Arc::new(SinkColetor::default());
        let provider = ClaudeCliProvider::new(sink.clone(), EgressClass::CloudOptOut, None)
            .with_binary("/caminho/que/nao/existe/claude");

        provider
            .chat(ChatRequest::new("m", vec![Message::user("oi")]))
            .await
            .expect_err("cloud-opt-out não alcança a nuvem da Anthropic");

        assert_eq!(
            sink.entradas.lock().expect("mutex sadio")[0].outcome,
            AuditOutcome::Blocked
        );
    }

    #[tokio::test]
    async fn erro_reportado_pelo_claude_nao_e_mascarado() {
        let corpo =
            r#"{"type":"result","subtype":"error","is_error":true,"result":"limite atingido"}"#;
        let (_dir, provider, _sink) = provider_com_script(corpo, EgressClass::CloudOk);

        let erro = provider
            .chat(ChatRequest::new("m", vec![Message::user("oi")]))
            .await
            .expect_err("is_error deve virar erro tratado");

        match erro {
            ProviderError::InvalidResponse(msg) => assert!(msg.contains("limite atingido")),
            outro => panic!("esperado InvalidResponse, veio {outro:?}"),
        }
    }

    #[tokio::test]
    async fn binario_ausente_da_erro_acionavel() {
        let sink = Arc::new(SinkColetor::default());
        let provider = ClaudeCliProvider::new(sink, EgressClass::CloudOk, None)
            .with_binary("/nao/existe/claude-xyz");

        let erro = provider
            .chat(ChatRequest::new("m", vec![Message::user("oi")]))
            .await
            .expect_err("binário ausente deve falhar");

        match erro {
            ProviderError::Network(msg) => assert!(
                msg.contains("não encontrado") && msg.contains("claude login"),
                "a mensagem deve dizer como resolver: {msg}"
            ),
            outro => panic!("esperado Network, veio {outro:?}"),
        }
    }

    #[tokio::test]
    async fn chat_stream_emite_start_delta_e_end_com_uso() {
        let (_dir, provider, _sink) = provider_com_script(SAIDA_OK, EgressClass::CloudOk);

        let mut stream = provider
            .chat_stream(ChatRequest::new("m", vec![Message::user("oi")]))
            .await
            .expect("stream deve iniciar");

        let mut eventos = Vec::new();
        while let Some(e) = stream.recv().await {
            eventos.push(e.expect("script não produz erro"));
        }

        assert_eq!(eventos.first(), Some(&StreamEvent::MessageStart));
        assert_eq!(
            eventos.last(),
            Some(&StreamEvent::MessageEnd {
                usage: Usage {
                    input_tokens: 12,
                    output_tokens: 3
                }
            })
        );
        let texto: String = eventos
            .iter()
            .filter_map(|e| match e {
                StreamEvent::TextDelta { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            texto, "olá",
            "sem duplicar o texto entre assistant e result"
        );
    }

    #[test]
    fn achatar_historico_poe_sistema_primeiro_e_rotula_papeis() {
        let messages = vec![
            Message::user("primeira"),
            Message::system("seja conciso"),
            Message::assistant("resposta"),
        ];

        let prompt = achatar_historico(&messages);

        assert!(
            prompt.starts_with("seja conciso"),
            "sistema vem primeiro: {prompt}"
        );
        assert!(prompt.contains("Usuário: primeira"));
        assert!(prompt.contains("Assistente: resposta"));
    }

    #[test]
    fn achatar_historico_omite_blocos_de_tool() {
        use crate::model::{ToolCall, ToolResult};

        let messages = vec![
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall(ToolCall {
                    id: "c1".into(),
                    name: "fs_read".into(),
                    arguments: serde_json::json!({}),
                })],
            },
            Message {
                role: Role::Tool,
                content: vec![ContentBlock::ToolResult(ToolResult {
                    call_id: "c1".into(),
                    content: "conteúdo".into(),
                    is_error: false,
                })],
            },
            Message::user("e agora?"),
        ];

        let prompt = achatar_historico(&messages);

        assert_eq!(
            prompt, "Usuário: e agora?",
            "blocos de tool não têm representação neste provider"
        );
    }

    #[test]
    fn linha_desconhecida_nunca_e_fatal() {
        let mut acc = SaidaAcumulada::default();
        aplicar_linha(r#"{"type":"tipo_que_nao_existe_ainda"}"#, &mut acc)
            .expect("tipo novo do Claude Code não pode quebrar o provider");
        aplicar_linha("isso não é json", &mut acc).expect("lixo na saída é ignorado");
        aplicar_linha("", &mut acc).expect("linha vazia é ignorada");
        assert!(acc.texto.is_empty());
    }
}
