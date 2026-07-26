use crate::domain::{ErDiagram, SaveErDiagramInput};
use crate::error::AppResult;
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

pub struct DiagramService<'a> {
    pub(crate) db: &'a AppDatabase,
}

impl DiagramService<'_> {
    pub fn list(&self, family: Option<&str>) -> AppResult<Vec<ErDiagram>> {
        self.db.with_conn(|c| repo::diagrams::list_all(c, family))
    }

    pub fn get(&self, id: &str) -> AppResult<ErDiagram> {
        self.db.with_conn(|c| repo::diagrams::get_by_id(c, id))
    }

    pub fn save(&self, input: SaveErDiagramInput) -> AppResult<ErDiagram> {
        self.db.with_conn(|c| repo::diagrams::save(c, input))
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        self.db.with_conn(|c| repo::diagrams::delete(c, id))
    }
}
