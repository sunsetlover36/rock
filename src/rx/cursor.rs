use mlua::UserData;
use std::cell::RefCell;

use crate::runtime::plugins::yield_op;

// TODO: Neynar API thing ({ next: { cursor: String }? })
// Might support more structures?

struct CursorState {
    exhausted: bool,
    op: mlua::Table,
}
impl CursorState {
    fn update_from_response(&mut self, response: &mlua::Table) -> mlua::Result<()> {
        let next: Option<mlua::Table> = response.get("next")?;
        match next {
            Some(next) => {
                let cursor: Option<String> = next.get("cursor")?;
                match cursor {
                    Some(cursor) if !cursor.is_empty() => {
                        let args: mlua::Table = self.op.get("args")?;
                        args.set("cursor", cursor)?;
                    }
                    _ => {
                        self.exhausted = true;
                    }
                }
            }
            None => {
                self.exhausted = true;
            }
        }

        Ok(())
    }
}

pub(crate) struct CursorRx {
    state: RefCell<CursorState>,
}
impl CursorRx {
    pub fn new(op: mlua::Table) -> Self {
        Self {
            state: RefCell::new(CursorState {
                exhausted: false,
                op,
            }),
        }
    }
}
impl UserData for CursorRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method_mut("next", async |lua, this, _: ()| {
            let exhausted = { this.state.borrow().exhausted };
            if exhausted {
                return Ok(mlua::Value::Nil);
            }

            let op = {
                let state = this.state.borrow();
                state.op.clone()
            };

            let response: mlua::Value = yield_op(&lua, "cursor.next", op).await?;
            let mlua::Value::Table(t) = &response else {
                return Err(mlua::Error::runtime("cursor next: expected table response"));
            };

            let mut state = this.state.borrow_mut();
            state.update_from_response(t)?;

            Ok(response)
        });
    }
}
