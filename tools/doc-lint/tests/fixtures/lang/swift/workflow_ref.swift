// case: a question id in a doc comment is reported

/// OQ3 decides
func decided() {}

// case: the initializer label is not reported, in code or in a comment

final class View: NSView {
    // init?(coder: NSCoder) is required by NSView
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is unused")
    }
}

// case: a pattern inside a string is not reported

let message = "the coder adds \(decided())"
