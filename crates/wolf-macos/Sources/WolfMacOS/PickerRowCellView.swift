//
//  PickerRowCellView.swift
//  A reusable, file-agnostic list-row view rendering a `PickerRow`.
//
//  Deliberately GENERAL (like `PickerRow`), so the same cell backs any picker /
//  selector: a leading `NSImageView` (the medium icon, aligned with the search
//  magnifier) and a single title label (aligned with the typed text). The icon's
//  BOTTOM lines up with the text's baseline. The alignment insets are derived by
//  `PickerView` from the search-bar layout and injected via `init`. AppKit-only,
//  so exercised by manual smoke.
//

import AppKit

/// A picker list row: icon on the left, a single line of text on the right.
final class PickerRowCellView: NSTableCellView {
    /// Side length of the (square) medium icon. Its left edge lines up with the
    /// search magnifier and its bottom with the text baseline.
    static let iconSize: CGFloat = 28
    /// Point size of the title label.
    static let titleFontSize: CGFloat = 15
    /// Horizontal gap between the icon and the text column (the padding to the
    /// right of the icon).
    static let iconToTextGap: CGFloat = 8
    /// Trailing inset of the text within the cell.
    static let cellHorizontalInset: CGFloat = 8
    /// How far the text baseline sits ABOVE the icon view's bottom edge, so it
    /// aligns with the icon glyph's visible bottom (Finder icon images carry a
    /// small transparent bottom margin below the drawn glyph).
    static let iconBaselineInset: CGFloat = 3

    /// The title font.
    static let titleFont = NSFont.systemFont(ofSize: titleFontSize)

    private let iconView = NSImageView()
    private let titleLabel = NSTextField(labelWithString: "")

    /// Builds the row's subviews and pins them using the caller-computed alignment
    /// insets: `iconLeadingInset` aligns the icon with the search magnifier and
    /// `labelLeadingInset` aligns the text with the typed text. The icon is
    /// vertically centered and its BOTTOM is pinned to the text's baseline.
    init(iconLeadingInset: CGFloat, labelLeadingInset: CGFloat) {
        super.init(frame: .zero)

        iconView.translatesAutoresizingMaskIntoConstraints = false
        iconView.imageScaling = .scaleProportionallyUpOrDown
        addSubview(iconView)

        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.font = Self.titleFont
        titleLabel.lineBreakMode = .byTruncatingMiddle
        titleLabel.drawsBackground = false
        titleLabel.textColor = .labelColor
        addSubview(titleLabel)

        self.imageView = iconView
        self.textField = titleLabel

        NSLayoutConstraint.activate([
            iconView.leadingAnchor.constraint(
                equalTo: leadingAnchor, constant: iconLeadingInset),
            iconView.centerYAnchor.constraint(equalTo: centerYAnchor),
            iconView.widthAnchor.constraint(equalToConstant: Self.iconSize),
            iconView.heightAnchor.constraint(equalToConstant: Self.iconSize),

            titleLabel.leadingAnchor.constraint(
                equalTo: leadingAnchor, constant: labelLeadingInset),
            titleLabel.trailingAnchor.constraint(
                lessThanOrEqualTo: trailingAnchor, constant: -Self.cellHorizontalInset),
            // The text baseline lines up with the icon glyph's visible bottom
            // (just above the icon view's bottom edge; see `iconBaselineInset`).
            titleLabel.firstBaselineAnchor.constraint(
                equalTo: iconView.bottomAnchor, constant: -Self.iconBaselineInset),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("PickerRowCellView is not constructed from a nib")
    }

    /// The fixed row height for the picker list: the icon (the taller element) plus
    /// vertical padding, which also leaves room for the text descenders that fall
    /// below the icon's (baseline-aligned) bottom edge.
    static func rowHeight(
        iconSize: CGFloat,
        titleFont: NSFont,
        verticalPadding: CGFloat
    ) -> CGFloat {
        let titleLineHeight = NSLayoutManager().defaultLineHeight(for: titleFont)
        return max(iconSize, titleLineHeight) + verticalPadding
    }

    /// Applies a `PickerRow`: sets the icon and the single line of text.
    func configure(with row: PickerRow) {
        iconView.image = row.icon
        titleLabel.stringValue = row.title
    }
}
