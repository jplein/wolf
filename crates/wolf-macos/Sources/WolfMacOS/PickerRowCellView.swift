//
//  PickerRowCellView.swift
//  A reusable, file-agnostic list-row view rendering a `PickerRow`.
//
//  Deliberately GENERAL (like `PickerRow`), so the same cell backs any picker /
//  selector: a leading `NSImageView` (the medium icon, aligned with the search
//  magnifier) and a vertical `NSStackView` of a title label (aligned with the
//  typed text) over a smaller, dimmer subtitle label. The alignment insets are
//  computed by `PickerView` from the search-field cell rects and injected via
//  `init`. AppKit-only, so exercised by manual smoke.
//

import AppKit

/// A picker list row: icon on the left, title over subtitle on the right.
final class PickerRowCellView: NSTableCellView {
    /// Side length of the (square) medium icon.
    static let iconSize: CGFloat = 28
    /// Point size of the primary title label.
    static let titleFontSize: CGFloat = 15
    /// Point size of the secondary subtitle label.
    static let subtitleFontSize: CGFloat = 11
    /// Horizontal gap between the icon and the text column.
    static let iconToTextGap: CGFloat = 8
    /// Vertical gap between the title and subtitle labels.
    static let interlineGap: CGFloat = 2
    /// Trailing inset of the text column within the cell.
    static let cellHorizontalInset: CGFloat = 8

    /// The primary title font.
    static let titleFont = NSFont.systemFont(ofSize: titleFontSize)
    /// The secondary subtitle font.
    static let subtitleFont = NSFont.systemFont(ofSize: subtitleFontSize)

    private let iconView = NSImageView()
    private let titleLabel = NSTextField(labelWithString: "")
    private let subtitleLabel = NSTextField(labelWithString: "")
    private let stack = NSStackView()

    /// Builds the row's subviews and pins them using the caller-computed alignment
    /// insets: `iconLeadingInset` aligns the icon with the search magnifier and
    /// `labelLeadingInset` aligns the title with the typed text (already clamped by
    /// the caller so the text never overlaps the icon).
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

        subtitleLabel.translatesAutoresizingMaskIntoConstraints = false
        subtitleLabel.font = Self.subtitleFont
        subtitleLabel.lineBreakMode = .byTruncatingMiddle
        subtitleLabel.drawsBackground = false
        subtitleLabel.textColor = .secondaryLabelColor

        stack.translatesAutoresizingMaskIntoConstraints = false
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = Self.interlineGap
        stack.addArrangedSubview(titleLabel)
        stack.addArrangedSubview(subtitleLabel)
        addSubview(stack)

        self.imageView = iconView
        self.textField = titleLabel

        NSLayoutConstraint.activate([
            iconView.leadingAnchor.constraint(
                equalTo: leadingAnchor, constant: iconLeadingInset),
            iconView.centerYAnchor.constraint(equalTo: centerYAnchor),
            iconView.widthAnchor.constraint(equalToConstant: Self.iconSize),
            iconView.heightAnchor.constraint(equalToConstant: Self.iconSize),

            stack.leadingAnchor.constraint(
                equalTo: leadingAnchor, constant: labelLeadingInset),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
            stack.trailingAnchor.constraint(
                lessThanOrEqualTo: trailingAnchor, constant: -Self.cellHorizontalInset),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("PickerRowCellView is not constructed from a nib")
    }

    /// The fixed row height for the picker list: the taller of the icon and the
    /// stacked title + subtitle line heights, plus vertical padding.
    static func rowHeight(
        iconSize: CGFloat,
        titleFont: NSFont,
        subtitleFont: NSFont,
        verticalPadding: CGFloat
    ) -> CGFloat {
        let layoutManager = NSLayoutManager()
        let titleLineHeight = layoutManager.defaultLineHeight(for: titleFont)
        let subtitleLineHeight = layoutManager.defaultLineHeight(for: subtitleFont)
        let textHeight = titleLineHeight + subtitleLineHeight + interlineGap
        return max(iconSize, textHeight) + verticalPadding
    }

    /// Applies a `PickerRow`: sets the icon, title, and subtitle (an empty
    /// subtitle hides the subtitle label, removing it from the stack view so the
    /// title centers vertically; the row height stays constant because the table
    /// uses a fixed `rowHeight`).
    func configure(with row: PickerRow) {
        iconView.image = row.icon
        titleLabel.stringValue = row.title
        subtitleLabel.stringValue = row.subtitle
        subtitleLabel.isHidden = row.subtitle.isEmpty
    }
}
