// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 of the License.

//! Position-addressable, editor-neutral facts for Evin configuration.

// Keep the files in one private namespace: they share helpers without exposing
// implementation details through the public semantic API.
include!("snapshot.rs");
include!("schema.rs");
include!("schema_resolution.rs");
include!("index.rs");
include!("session.rs");
include!("token_facts.rs");
include!("document_index.rs");
include!("reference_validation.rs");
include!("completion.rs");
include!("tests.rs");
