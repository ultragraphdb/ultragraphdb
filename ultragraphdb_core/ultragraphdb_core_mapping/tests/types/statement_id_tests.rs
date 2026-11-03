/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_mapping::{MappingError, StatementId};
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_a_statement_id_is_a_system_id_of_the_statement_type() -> TestResult {
    let subject = SystemId::new(TypePrefix::RESOURCE, Bucket::new(42)?, Sequence::new(9)?);
    let id = SystemId::statement(subject, Sequence::new(1)?);
    assert_eq!(StatementId::new(id)?.id(), id);
    Ok(())
}

#[test]
fn test_an_id_of_another_type_is_refused() -> TestResult {
    for prefix in [1u16, 3, u16::MAX] {
        let id = SystemId::new(
            TypePrefix::new(prefix)?,
            Bucket::new(42)?,
            Sequence::new(9)?,
        );
        assert_eq!(
            StatementId::new(id),
            Err(MappingError::not_a_statement_id(id))
        );
    }
    Ok(())
}
