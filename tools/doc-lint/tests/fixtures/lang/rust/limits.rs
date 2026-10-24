// case: a two-line block passes and a three-line block fails comment-lines

// two one
// two two
fn two() {}

// three one
// three two
// three three
fn three() {}

// case: blank comment lines count toward the line limit

//
// middle
//
fn blank() {}

// case: exactly 100 chars passes and 101 chars fails comment-width

// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10ab
fn hundred() {}

// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abc
fn hundred_one() {}

// case: width is measured from the marker, so deep indentation is free

fn deep() {
                                        // seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10ab
}

// case: width counts chars, so multi-byte em dashes stay within 100

// seg01———— seg02———— seg03———— seg04———— seg05———— seg06———— seg07———— seg08———— seg09———— seg10——
fn dashes() {}

// case: a 101-char line in a two-line block reports only comment-width

// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abc
// second line of that block

// case: a single whitespace-free token is exempt from the width limit

// https://example.com/a-very-long-path/segment-0001/segment-0002/segment-0003/segment-0004/segment-0005/segment-0006/segment-0007/index.html
fn url() {}

// case: prose around a long URL is not exempt and fails comment-width

// See https://example.com/a-very-long-path/segment-0001/segment-0002/segment-0003/segment-0004/segment-0005/segment-0006/segment-0007/index.html
fn see_url() {}
