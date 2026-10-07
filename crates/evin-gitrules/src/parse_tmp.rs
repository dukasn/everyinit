// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

#[test]
#[ignore]
fn test_parse_tmp() {
    let path = std::path::PathBuf::from("/tmp/evin-test-repo/.evin/gitrules.flow");
    let result = crate::parse_file(&path);
    println!("RESULT: {:#?}", result);
    assert!(result.is_ok());
}
