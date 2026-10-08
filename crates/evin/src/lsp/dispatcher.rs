// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 of the License.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use super::protocol::{LspError, LspNotification, LspRequest};
use super::server::EvinLspServer;
use evin_lsp::protocol::{JsonRpcNotification, JsonRpcRequest};
use serde_json::Value;
use std::io::{self, Write};

pub struct RequestDispatcher<'a, W> {
    request: JsonRpcRequest,
    server: &'a mut EvinLspServer,
    writer: &'a mut W,
    handled: bool,
}

impl<'a, W: Write> RequestDispatcher<'a, W> {
    pub fn new(request: JsonRpcRequest, server: &'a mut EvinLspServer, writer: &'a mut W) -> Self {
        Self {
            request,
            server,
            writer,
            handled: false,
        }
    }

    pub fn on<R, H>(mut self, handler: H) -> io::Result<Self>
    where
        R: LspRequest,
        H: FnOnce(&mut EvinLspServer, R::Params) -> Result<R::Result, LspError>,
    {
        if self.handled || self.request.method != R::METHOD {
            return Ok(self);
        }
        self.handled = true;
        let params_val = self.request.params.take().unwrap_or(Value::Null);
        let id = self.request.id.clone();

        let parsed_params =
            serde_json::from_value::<R::Params>(params_val.clone()).or_else(|err| {
                if params_val.is_null() {
                    serde_json::from_value::<R::Params>(Value::Object(Default::default()))
                } else {
                    Err(err)
                }
            });

        match parsed_params {
            Ok(params) => match handler(self.server, params) {
                Ok(result) => {
                    self.server.respond(self.writer, id, &result)?;
                    if self.server.needs_initial_rebuild {
                        self.server.needs_initial_rebuild = false;
                        self.server.rebuild_and_publish(self.writer)?;
                    }
                }
                Err(err) => {
                    self.server
                        .respond_error(self.writer, id, err.code, err.message)?;
                }
            },
            Err(err) => {
                self.server.respond_error(
                    self.writer,
                    id,
                    LspError::INVALID_PARAMS,
                    err.to_string(),
                )?;
            }
        }
        Ok(self)
    }

    pub fn finish(self) -> io::Result<()> {
        if !self.handled {
            let id = self.request.id;
            let method = self.request.method;
            self.server.respond_error(
                self.writer,
                id,
                LspError::METHOD_NOT_FOUND,
                format!("Method '{method}' not found"),
            )?;
        }
        Ok(())
    }
}

pub struct NotificationDispatcher<'a, W> {
    notification: JsonRpcNotification,
    server: &'a mut EvinLspServer,
    writer: &'a mut W,
    handled: bool,
}

impl<'a, W: Write> NotificationDispatcher<'a, W> {
    pub fn new(
        notification: JsonRpcNotification,
        server: &'a mut EvinLspServer,
        writer: &'a mut W,
    ) -> Self {
        Self {
            notification,
            server,
            writer,
            handled: false,
        }
    }

    pub fn on<N, H>(mut self, handler: H) -> io::Result<Self>
    where
        N: LspNotification,
        H: FnOnce(&mut EvinLspServer, N::Params, &mut W) -> io::Result<()>,
    {
        if self.handled || self.notification.method != N::METHOD {
            return Ok(Self {
                notification: self.notification,
                server: self.server,
                writer: self.writer,
                handled: self.handled,
            });
        }
        self.handled = true;
        let params_val = self.notification.params.take().unwrap_or(Value::Null);
        let parsed_params =
            serde_json::from_value::<N::Params>(params_val.clone()).or_else(|err| {
                if params_val.is_null() {
                    serde_json::from_value::<N::Params>(Value::Object(Default::default()))
                } else {
                    Err(err)
                }
            });
        if let Ok(params) = parsed_params {
            handler(self.server, params, self.writer)?;
        }
        Ok(self)
    }

    pub fn finish(self) -> io::Result<()> {
        Ok(())
    }
}
