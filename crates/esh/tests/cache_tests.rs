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

use esh::cache::PlanCache;
use std::sync::Arc;

#[test]
fn test_plan_cache_hit_and_miss() {
    let cache = PlanCache::new();
    assert_eq!(cache.len(), 0);

    let script = "mkdir -p ./dist &| cargo build";

    // Cache miss: compiles and inserts
    let plan1 = cache.get_or_compile(script).expect("compilation failed");
    assert_eq!(cache.len(), 1);

    // Cache hit: returns identical Arc pointer
    let plan2 = cache.get_or_compile(script).expect("compilation failed");
    assert_eq!(cache.len(), 1);
    assert!(Arc::ptr_eq(&plan1, &plan2));

    // Different script: compiles new plan
    let script2 = "rm -rf ./dist";
    let plan3 = cache.get_or_compile(script2).expect("compilation failed");
    assert_eq!(cache.len(), 2);
    assert!(!Arc::ptr_eq(&plan1, &plan3));

    // Clear cache
    cache.clear();
    assert_eq!(cache.len(), 0);
}
