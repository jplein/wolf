// case: an outer doc comment citing a plan is reported

/// Falsifying edit (from the plan): x
fn outer() {}

// case: an inner doc comment citing a plan by section sign is reported

//! per plan §3

// case: a role instruction in capitals is reported

// NOTE TO CODER: y
fn capitals() {}

// case: a decision id is reported

// see OD2.
fn decision() {}

// case: a role name in a block comment is reported

/* the test-writer adds */
fn block() {}

// case: each remaining phrase is reported, one comment each

// a note to the coder: z

// ask the coder

// a test writer adds it

// an architect plan says so

// ask the architect

// sorted per plan

// kept per the plan

// in the feature plan

// see plan §4

// OQ12 decides

fn trailing() {} // the fix plan step

// case: a doc block comment is scanned too

/** the architect decides */
fn doc_block() {}

// case: the finding is on the matching line of a block comment

/*
   the first line is clean
   test-writer fills it
*/
fn multi() {}

// case: each matching line of a line-comment block is its own finding

// the coder adds it
// and OQ5 checks it
fn per_line() {}

// case: two phrases on one line give one finding

// the coder runs it per the plan
fn twice() {}

// case: a pattern inside a string is not reported

fn strings() -> &'static str {
    let s = "per plan §3";
    s
}

// case: look-alikes without word boundaries are not reported

// COD2 ODE OD restore plan architecture Information Architects

// the restore plan's tabs

// see the architecture overview

// OD2B and OQ3x are part numbers

// lathe coder settings

// the_coder and OD2_x are identifiers
fn lookalikes() {}

// case: a hyphenated decision or question id is reported, in any case

// see OQ-1.

/// plan OD-a picks …
fn hyphen_doc() {}

// (od-4)

// case: a hyphen before the id is a word boundary, so the id is reported

// the method-OQ-1 suffix
fn hyphen_ids() {}

// case: two comments on one line give one finding

/* a */ /* the coder */

/* the coder */ /* per plan */
fn two_comments() {}

// case: hyphenated look-alikes are not reported

// COD-1

// OD- then a space

// it ends with OD-

// OQ-_x

// OQ-1_x and OD-a_b are identifiers
fn hyphen_lookalikes() {}

// case: a section reference and the role plurals are reported

// (plan section 3.2)

/// see Plan Section 2.4
fn section_doc() {}

// the test-writers agreed

// two test writers

// as the coders found
fn plurals() {}

// case: plural and section look-alikes are not reported

// the coderset

// test-writersx

// a planned section
fn plural_lookalikes() {}

// case: a section plural fails the trailing word boundary, so it is not reported

// the floor plan sections
fn section_plural() {}
