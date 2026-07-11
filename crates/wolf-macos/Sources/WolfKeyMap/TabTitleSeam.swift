//
//  TabTitleSeam.swift
//  Pure, AppKit-free per-tab title computation seam.
//
//  Computes the titles for the tabs in ONE window from their active buffer
//  paths (plan Phase 3 + OD5). A pure, display-free seam exercised with scalar
//  string inputs only — NO AppKit, NO window, NO live Neovim — so it lives in
//  the Foundation-only `WolfKeyMap` module alongside `CommandSeam.swift` /
//  `PaneTree.swift`.
//
//  CONTRACT:
//    * 1:1 and ORDER-PRESERVING: result[i] is the label for
//      activeBufferPaths[i]; same count + order as the input.
//    * Each label starts as the path's basename (last "/"-separated component).
//    * Empty string, "[No Name]", and other unnamed inputs => "untitled".
//      Untitled entries are EXCLUDED from the collision/disambiguation pass
//      (a pathless tab cannot be disambiguated by a path).
//    * When two or more REAL-path tabs share a basename, each colliding label is
//      extended to "<basename> — <minimal trailing parent segments>" (EM DASH
//      U+2014, one space each side). The trailing-segment count grows minimally
//      until every label in that colliding set is distinct (or the paths are
//      exhausted); segments are joined path-order with "/".
//

import Foundation

/// Computes the per-tab titles for the tabs in one window from their active
/// buffer paths (see the file header for the exact contract).
public enum TabTitleSeam {
    /// The label for an unnamed / empty tab.
    private static let untitled = "untitled"

    /// The disambiguation separator: a space, an EM DASH (U+2014), a space.
    private static let separator = " \u{2014} "

    /// The unnamed-buffer sentinel Neovim reports for a scratch/`:enew` buffer.
    private static let noNameSentinel = "[No Name]"

    /// Maps each active buffer path to its tab label, order-preserving.
    public static func labels(activeBufferPaths: [String]) -> [String] {
        // Parse every input into either "unnamed" or a named path split into its
        // "/"-separated segments (root-first; the last segment is the basename).
        let segmentsPerIndex: [[String]?] = activeBufferPaths.map { raw in
            if raw.isEmpty || raw == noNameSentinel {
                return nil
            }
            let segs = raw.split(separator: "/", omittingEmptySubsequences: true).map(String.init)
            return segs.isEmpty ? nil : segs
        }

        // Group the NAMED indices by basename; unnamed indices are excluded from
        // the disambiguation pass entirely.
        var indicesByBasename: [String: [Int]] = [:]
        for (i, segs) in segmentsPerIndex.enumerated() {
            guard let segs, let basename = segs.last else { continue }
            indicesByBasename[basename, default: []].append(i)
        }

        var result = [String](repeating: untitled, count: activeBufferPaths.count)

        for (basename, indices) in indicesByBasename {
            if indices.count == 1 {
                result[indices[0]] = basename
                continue
            }
            for (idx, label) in disambiguated(basename: basename, indices: indices, segmentsPerIndex: segmentsPerIndex) {
                result[idx] = label
            }
        }

        return result
    }

    /// Disambiguates a colliding basename group by growing the trailing parent
    /// segments minimally until the labels are all distinct (or paths run out).
    private static func disambiguated(
        basename: String,
        indices: [Int],
        segmentsPerIndex: [[String]?]
    ) -> [Int: String] {
        // The deepest member's parent count bounds how far we can grow.
        let maxParents = indices.map { (segmentsPerIndex[$0]?.count ?? 1) - 1 }.max() ?? 0

        var take = 1
        var labels: [Int: String] = [:]
        while true {
            labels.removeAll(keepingCapacity: true)
            for idx in indices {
                let segs = segmentsPerIndex[idx] ?? []
                let parents = segs.dropLast()
                let trailing = parents.suffix(min(take, parents.count))
                let parentStr = trailing.joined(separator: "/")
                labels[idx] = parentStr.isEmpty ? basename : basename + separator + parentStr
            }
            let values = Array(labels.values)
            if Set(values).count == values.count || take >= maxParents {
                break
            }
            take += 1
        }
        return labels
    }
}
