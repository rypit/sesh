//! The feeds store (specs/006 data-model.md, ADR 0009): which feeds each
//! owner has. Only the gateway writes it, for the operator's `sesh feed`
//! commands and for agent requests a person approved. Feed content never
//! reaches it.

use anyhow::Result;
use std::sync::Mutex;
use tokio_postgres::NoTls;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feed {
    pub owner: String,
    pub name: String,
    pub url: String,
    /// `operator`, or the agent whose approved request added it.
    pub added_by: String,
    pub added_at: u64,
}

/// Why an add was refused (the store's own rules).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddError {
    Exists(String),
    Full(u32),
    Unavailable,
}

impl std::fmt::Display for AddError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AddError::Exists(n) => write!(f, "a feed named {n} already exists"),
            AddError::Full(n) => write!(f, "this owner already has {n} feeds"),
            AddError::Unavailable => f.write_str("the feeds store is unavailable"),
        }
    }
}

pub enum Store {
    Postgres {
        url: String,
        client: tokio::sync::Mutex<Option<tokio_postgres::Client>>,
    },
    Memory {
        rows: Mutex<Vec<Feed>>,
        broken: bool,
    },
}

const COLS: &str = "owner, name, url, added_by, added_at";

fn row(r: &tokio_postgres::Row) -> Feed {
    Feed {
        owner: r.get(0),
        name: r.get(1),
        url: r.get(2),
        added_by: r.get(3),
        added_at: r.get::<_, i64>(4) as u64,
    }
}

impl Store {
    pub fn postgres(url: String) -> Self {
        Store::Postgres {
            url,
            client: tokio::sync::Mutex::new(None),
        }
    }

    pub fn memory() -> Self {
        Store::Memory {
            rows: Mutex::default(),
            broken: false,
        }
    }

    pub fn broken() -> Self {
        Store::Memory {
            rows: Mutex::default(),
            broken: true,
        }
    }

    /// Runs `f` on a connected client; a closed connection is dropped so
    /// the next call reconnects.
    async fn with<T>(
        url: &str,
        client: &tokio::sync::Mutex<Option<tokio_postgres::Client>>,
        f: impl AsyncFnOnce(&tokio_postgres::Client) -> Result<T, tokio_postgres::Error>,
    ) -> Result<T> {
        let mut g = client.lock().await;
        if g.is_none() {
            let (c, conn) = tokio_postgres::connect(url, NoTls).await?;
            tokio::spawn(async move {
                let _ = conn.await;
            });
            *g = Some(c);
        }
        let r = f(g.as_ref().expect("connected")).await;
        if let Err(e) = &r
            && e.is_closed()
        {
            *g = None;
        }
        Ok(r?)
    }

    /// Every feed, ordered by owner and name.
    pub async fn all(&self) -> Result<Vec<Feed>> {
        match self {
            Store::Memory { rows, broken } => {
                anyhow::ensure!(!broken, "feeds store unavailable");
                let mut v = rows.lock().expect("lock").clone();
                v.sort_by(|a, b| (&a.owner, &a.name).cmp(&(&b.owner, &b.name)));
                Ok(v)
            }
            Store::Postgres { url, client } => {
                let rows = Self::with(url, client, async |c| {
                    c.query(
                        &format!("SELECT {COLS} FROM feeds ORDER BY owner, name"),
                        &[],
                    )
                    .await
                })
                .await?;
                Ok(rows.iter().map(row).collect())
            }
        }
    }

    /// The feeds of these owners, in the owners' order (own first).
    pub async fn readable(&self, owners: &[String]) -> Result<Vec<Feed>> {
        let all = self.all().await?;
        Ok(owners
            .iter()
            .flat_map(|o| all.iter().filter(move |f| &f.owner == o).cloned())
            .collect())
    }

    /// Adds a feed, unless the owner has one by that name or is at `max`.
    pub async fn add(&self, f: &Feed, max: u32) -> Result<(), AddError> {
        let mine = self
            .readable(std::slice::from_ref(&f.owner))
            .await
            .map_err(|_| AddError::Unavailable)?;
        if mine.iter().any(|x| x.name == f.name) {
            return Err(AddError::Exists(f.name.clone()));
        }
        if mine.len() as u32 >= max {
            return Err(AddError::Full(max));
        }
        match self {
            Store::Memory { rows, broken } => {
                if *broken {
                    return Err(AddError::Unavailable);
                }
                rows.lock().expect("lock").push(f.clone());
                Ok(())
            }
            Store::Postgres { url, client } => {
                let f = f.clone();
                let n = Self::with(url, client, async |c| {
                    c.execute(
                        &format!(
                            "INSERT INTO feeds ({COLS}) VALUES ($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING"
                        ),
                        &[&f.owner, &f.name, &f.url, &f.added_by, &(f.added_at as i64)],
                    )
                    .await
                })
                .await
                .map_err(|_| AddError::Unavailable)?;
                if n == 0 {
                    return Err(AddError::Exists(f.name));
                }
                Ok(())
            }
        }
    }

    /// Removes a feed; false if there was none.
    pub async fn remove(&self, owner: &str, name: &str) -> Result<bool> {
        match self {
            Store::Memory { rows, broken } => {
                anyhow::ensure!(!broken, "feeds store unavailable");
                let mut r = rows.lock().expect("lock");
                let before = r.len();
                r.retain(|f| !(f.owner == owner && f.name == name));
                Ok(r.len() < before)
            }
            Store::Postgres { url, client } => {
                let n = Self::with(url, client, async |c| {
                    c.execute(
                        "DELETE FROM feeds WHERE owner = $1 AND name = $2",
                        &[&owner, &name],
                    )
                    .await
                })
                .await?;
                Ok(n > 0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(owner: &str, name: &str) -> Feed {
        Feed {
            owner: owner.into(),
            name: name.into(),
            url: format!("https://example.org/{name}.xml"),
            added_by: "operator".into(),
            added_at: 1,
        }
    }

    #[tokio::test]
    async fn adds_lists_and_removes_per_owner() {
        let s = Store::memory();
        s.add(&feed("household", "harbour"), 50).await.unwrap();
        s.add(&feed("ana", "garden"), 50).await.unwrap();
        s.add(&feed("ben", "cycling"), 50).await.unwrap();
        // The same name under another owner is a different feed.
        s.add(&feed("ana", "harbour"), 50).await.unwrap();
        assert_eq!(
            s.add(&feed("ana", "garden"), 50).await,
            Err(AddError::Exists("garden".into()))
        );
        let ana: Vec<(String, String)> = s
            .readable(&["ana".into(), "household".into()])
            .await
            .unwrap()
            .into_iter()
            .map(|f| (f.owner, f.name))
            .collect();
        assert_eq!(
            ana,
            vec![
                ("ana".into(), "garden".into()),
                ("ana".into(), "harbour".into()),
                ("household".into(), "harbour".into())
            ],
            "own feeds first, and never ben's"
        );
        let house = s.readable(&["household".into()]).await.unwrap();
        assert_eq!(house.len(), 1);
        assert!(s.remove("ana", "garden").await.unwrap());
        assert!(!s.remove("ana", "garden").await.unwrap());
        assert!(!s.remove("ben", "harbour").await.unwrap());
        assert_eq!(s.all().await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn per_owner_limit_and_broken_store() {
        let s = Store::memory();
        s.add(&feed("ana", "a"), 2).await.unwrap();
        s.add(&feed("ana", "b"), 2).await.unwrap();
        assert_eq!(s.add(&feed("ana", "c"), 2).await, Err(AddError::Full(2)));
        s.add(&feed("ben", "c"), 2).await.unwrap();
        let b = Store::broken();
        assert_eq!(
            b.add(&feed("ana", "a"), 2).await,
            Err(AddError::Unavailable)
        );
        assert!(b.all().await.is_err());
    }
}
