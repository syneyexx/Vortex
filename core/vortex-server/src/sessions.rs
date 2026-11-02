//! Sessies en berichten (opslaan/herstellen, forken, geschiedenis). Elke wijziging + event in
//! één transactie (outbox).
// VX:W03.13

use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use vortex_core::{CoreError, EventLog};
use vortex_types::{Event, EventKind, MessageId, RunId, SessionId, TraceId};

pub type Result<T> = std::result::Result<T, CoreError>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub title: String,
    pub parent_session_id: Option<SessionId>,
    pub forked_from_message: Option<MessageId>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub message_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: MessageId,
    pub session_id: SessionId,
    pub ord: i64,
    pub role: String,
    pub content: String,
    pub reasoning: String,
    pub tool_calls: serde_json::Value,
    pub run_id: Option<RunId>,
    pub model: Option<String>,
    pub mode: Option<String>,
    pub config_version: Option<String>,
    pub status: String,
    pub metrics: Option<serde_json::Value>,
    pub created_at: String,
}

fn bad(e: impl std::fmt::Display) -> CoreError {
    CoreError::Config(e.to_string())
}

type SessionRow = (String, String, Option<String>, Option<String>, String, String, i64);

fn row_session(r: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))
}

fn to_session(t: SessionRow) -> Result<Session> {
    Ok(Session {
        id: t.0.parse().map_err(bad)?,
        title: t.1,
        parent_session_id: t.2.map(|p| p.parse()).transpose().map_err(bad)?,
        forked_from_message: t.3.map(|p| p.parse()).transpose().map_err(bad)?,
        created_at: t.4,
        updated_at: t.5,
        message_count: t.6,
    })
}

const SEL_SESSION: &str =
    "SELECT s.id, s.title, s.parent_session_id, s.forked_from_message, s.created_at, s.updated_at,
    (SELECT COUNT(*) FROM messages m WHERE m.session_id = s.id) FROM sessions s";

const SEL_MSG: &str =
    "SELECT id, session_id, ord, role, content, reasoning, tool_calls, run_id, model, mode, config_version,
    status, metrics, created_at FROM messages";

fn row_msg(r: &rusqlite::Row<'_>) -> rusqlite::Result<StoredMessageRow> {
    Ok(StoredMessageRow {
        id: r.get(0)?,
        session_id: r.get(1)?,
        ord: r.get(2)?,
        role: r.get(3)?,
        content: r.get(4)?,
        reasoning: r.get(5)?,
        tool_calls: r.get(6)?,
        run_id: r.get(7)?,
        model: r.get(8)?,
        mode: r.get(9)?,
        config_version: r.get(10)?,
        status: r.get(11)?,
        metrics: r.get(12)?,
        created_at: r.get(13)?,
    })
}

#[derive(Debug)]
struct StoredMessageRow {
    id: String,
    session_id: String,
    ord: i64,
    role: String,
    content: String,
    reasoning: String,
    tool_calls: String,
    run_id: Option<String>,
    model: Option<String>,
    mode: Option<String>,
    config_version: Option<String>,
    status: String,
    metrics: Option<String>,
    created_at: String,
}

impl StoredMessageRow {
    fn into_msg(self) -> Result<StoredMessage> {
        Ok(StoredMessage {
            id: self.id.parse().map_err(bad)?,
            session_id: self.session_id.parse().map_err(bad)?,
            ord: self.ord,
            role: self.role,
            content: self.content,
            reasoning: self.reasoning,
            tool_calls: serde_json::from_str(&self.tool_calls)?,
            run_id: self.run_id.map(|r| r.parse()).transpose().map_err(bad)?,
            model: self.model,
            mode: self.mode,
            config_version: self.config_version,
            status: self.status,
            metrics: self.metrics.map(|m| serde_json::from_str(&m)).transpose()?,
            created_at: self.created_at,
        })
    }
}

/// Nieuw bericht.
#[derive(Debug, Clone, Default)]
pub struct NewMessage {
    pub role: String,
    pub content: String,
    pub run_id: Option<RunId>,
    pub model: Option<String>,
    pub mode: Option<String>,
    pub config_version: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct SessionStore {
    log: EventLog,
}

impl SessionStore {
    pub fn new(log: EventLog) -> Self {
        Self { log }
    }

    pub fn create(&self, title: &str) -> Result<Session> {
        let id = SessionId::new();
        let now = vortex_types::now().to_rfc3339();
        self.log.transact(|tx, out| {
            tx.execute(
                "INSERT INTO sessions(id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
                params![id.to_string(), title, now],
            )?;
            out.emit(
                tx,
                Event::new(
                    EventKind::SessionCreated,
                    TraceId::new(),
                    serde_json::json!({"session_id": id, "title": title}),
                ),
            )?;
            Ok(())
        })?;
        self.get(id)
    }

    pub fn get(&self, id: SessionId) -> Result<Session> {
        self.log.db().read(|c| {
            let t = c
                .query_row(&format!("{SEL_SESSION} WHERE s.id = ?1"), [id.to_string()], row_session)
                .optional()?
                .ok_or_else(|| CoreError::NotFound(id.to_string()))?;
            to_session(t)
        })
    }

    /// Geschiedenis: nieuwste eerst.
    pub fn list(&self, limit: usize) -> Result<Vec<Session>> {
        self.log.db().read(|c| {
            let mut st =
                c.prepare(&format!("{SEL_SESSION} WHERE s.archived = 0 ORDER BY s.updated_at DESC LIMIT ?1"))?;
            let rows = st.query_map([limit as i64], row_session)?.collect::<rusqlite::Result<Vec<_>>>()?;
            rows.into_iter().map(to_session).collect()
        })
    }

    pub fn rename(&self, id: SessionId, title: &str) -> Result<()> {
        self.log.transact(|tx, out| {
            let n = tx.execute(
                "UPDATE sessions SET title = ?2, updated_at = ?3 WHERE id = ?1",
                params![id.to_string(), title, vortex_types::now().to_rfc3339()],
            )?;
            if n == 0 {
                return Err(CoreError::NotFound(id.to_string()));
            }
            out.emit(
                tx,
                Event::new(
                    EventKind::SessionUpdated,
                    TraceId::new(),
                    serde_json::json!({"session_id": id, "title": title}),
                ),
            )?;
            Ok(())
        })
    }

    pub fn archive(&self, id: SessionId) -> Result<()> {
        self.log.transact(|tx, out| {
            tx.execute("UPDATE sessions SET archived = 1 WHERE id = ?1", [id.to_string()])?;
            out.emit(
                tx,
                Event::new(
                    EventKind::SessionUpdated,
                    TraceId::new(),
                    serde_json::json!({"session_id": id, "archived": true}),
                ),
            )?;
            Ok(())
        })
    }

    /// Berichten van een sessie. Berichten die nog streamen worden aangevuld uit de deltas in het
    /// event log, zodat herladen tijdens streamen niets verliest.
    pub fn messages(&self, id: SessionId) -> Result<Vec<StoredMessage>> {
        let mut msgs: Vec<StoredMessage> = self.log.db().read(|c| {
            let mut st = c.prepare(&format!("{SEL_MSG} WHERE session_id = ?1 ORDER BY ord"))?;
            let rows = st.query_map([id.to_string()], row_msg)?.collect::<rusqlite::Result<Vec<_>>>()?;
            rows.into_iter().map(StoredMessageRow::into_msg).collect()
        })?;
        for m in msgs.iter_mut().filter(|m| m.status == "streaming") {
            if let Some(run) = m.run_id {
                let (t, r) = self.deltas(run, m.id)?;
                m.content = t;
                m.reasoning = r;
            }
        }
        Ok(msgs)
    }

    /// Tekst + redenering van een bericht uit de gecoalesceerde deltas.
    fn deltas(&self, run: RunId, msg: MessageId) -> Result<(String, String)> {
        let (mut text, mut reasoning) = (String::new(), String::new());
        let id = serde_json::json!(msg);
        for e in self.log.for_run(run)? {
            if e.kind == EventKind::MessageDelta && e.payload["message_id"] == id {
                text.push_str(e.payload["text"].as_str().unwrap_or_default());
                reasoning.push_str(e.payload["reasoning"].as_str().unwrap_or_default());
            }
        }
        Ok((text, reasoning))
    }

    pub fn message(&self, id: MessageId) -> Result<StoredMessage> {
        self.log.db().read(|c| {
            c.query_row(&format!("{SEL_MSG} WHERE id = ?1"), [id.to_string()], row_msg)
                .optional()?
                .ok_or_else(|| CoreError::NotFound(id.to_string()))?
                .into_msg()
        })
    }

    /// Bericht toevoegen binnen een bestaande transactie (zodat het samen met events valt).
    pub fn insert_tx(tx: &Transaction<'_>, session: SessionId, m: &NewMessage) -> Result<MessageId> {
        let id = MessageId::new();
        let ord: i64 = tx.query_row(
            "SELECT COALESCE(MAX(ord), 0) + 1 FROM messages WHERE session_id = ?1",
            [session.to_string()],
            |r| r.get(0),
        )?;
        let now = vortex_types::now().to_rfc3339();
        tx.execute(
            "INSERT INTO messages(id, session_id, ord, role, content, run_id, model, mode, config_version, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id.to_string(),
                session.to_string(),
                ord,
                m.role,
                m.content,
                m.run_id.map(|r| r.to_string()),
                m.model,
                m.mode,
                m.config_version,
                m.status,
                now
            ],
        )?;
        tx.execute("UPDATE sessions SET updated_at = ?2 WHERE id = ?1", params![session.to_string(), now])?;
        Ok(id)
    }

    /// Bericht afronden (inhoud + status + metingen) binnen een transactie.
    pub fn finish_tx(
        tx: &Transaction<'_>,
        id: MessageId,
        content: &str,
        reasoning: &str,
        status: &str,
        metrics: Option<&serde_json::Value>,
    ) -> Result<()> {
        tx.execute(
            "UPDATE messages SET content = ?2, reasoning = ?3, status = ?4, metrics = ?5 WHERE id = ?1",
            params![id.to_string(), content, reasoning, status, metrics.map(|m| m.to_string())],
        )?;
        Ok(())
    }

    /// Fork: nieuwe sessie met alle berichten t/m `at` (branching).
    pub fn fork(&self, source: SessionId, at: MessageId) -> Result<Session> {
        let src = self.get(source)?;
        let msgs = self.messages(source)?;
        let cut = msgs.iter().position(|m| m.id == at).ok_or_else(|| CoreError::NotFound(at.to_string()))?;
        let id = SessionId::new();
        let now = vortex_types::now().to_rfc3339();
        self.log.transact(|tx, out| {
            tx.execute(
                "INSERT INTO sessions(id, title, parent_session_id, forked_from_message, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![id.to_string(), format!("{} (fork)", src.title), source.to_string(), at.to_string(), now],
            )?;
            for m in &msgs[..=cut] {
                tx.execute(
                    "INSERT INTO messages(id, session_id, ord, role, content, reasoning, tool_calls, run_id, model, mode,
                        config_version, status, metrics, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                    params![
                        MessageId::new().to_string(),
                        id.to_string(),
                        m.ord,
                        m.role,
                        m.content,
                        m.reasoning,
                        m.tool_calls.to_string(),
                        m.run_id.map(|r| r.to_string()),
                        m.model,
                        m.mode,
                        m.config_version,
                        if m.status == "streaming" { "cancelled" } else { m.status.as_str() },
                        m.metrics.as_ref().map(|x| x.to_string()),
                        m.created_at
                    ],
                )?;
            }
            out.emit(
                tx,
                Event::new(EventKind::SessionForked, TraceId::new(), serde_json::json!({"session_id": id, "from": source, "at": at})),
            )?;
            Ok(())
        })?;
        self.get(id)
    }

    /// Na een crash: berichten die nog 'streaming' staan, herstellen uit de deltas in het event log.
    pub fn repair_streaming(&self, runs: &vortex_core::RunStore) -> Result<usize> {
        let stuck: Vec<StoredMessage> = self.log.db().read(|c| {
            let mut st = c.prepare(&format!("{SEL_MSG} WHERE status = 'streaming'"))?;
            let rows = st.query_map([], row_msg)?.collect::<rusqlite::Result<Vec<_>>>()?;
            rows.into_iter().map(StoredMessageRow::into_msg).collect()
        })?;
        let mut n = 0;
        for m in stuck {
            let Some(run) = m.run_id else { continue };
            let rec = runs.get(run)?;
            if !rec.state.is_terminal() {
                continue;
            }
            let (text, reasoning) = self.deltas(run, m.id)?;
            let status = match rec.state {
                vortex_types::RunState::Completed => "complete",
                vortex_types::RunState::Cancelled => "cancelled",
                _ => "failed",
            };
            self.log.transact(|tx, out| {
                Self::finish_tx(tx, m.id, &text, &reasoning, status, None)?;
                out.emit(
                    tx,
                    Event::new(
                        EventKind::MessageCompleted,
                        rec.trace_id,
                        serde_json::json!({"session_id": m.session_id, "message_id": m.id, "status": status, "repaired": true}),
                    )
                    .for_run(run),
                )?;
                Ok(())
            })?;
            n += 1;
        }
        Ok(n)
    }
}
