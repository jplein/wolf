# macOS follow-ups

Last verified at commit `ab05119c2142fb544d8e3e87abddce0145a5fec0`.

This file holds the cross-platform findings that the task which found them could not close: a divergence between the two front-ends, or a suspected defect on one platform found while working on the other.

New findings are rare. Every task runs `./gates.sh` on a macOS and a Linux worker ([The verification gate](../build.md#the-verification-gate)), so a change is built and tested on both platforms before it lands. A finding lands here only when closing it needs a decision or a check that no gate makes.

## Ledger

## Adding and closing a finding

Give each finding its own `###` section under the ledger, headed by the date it was found and its subject. Under it, say what was found, whether it was measured or found by reading, the document that owns each file involved, and what closes it.

A finding closes when it is fixed under the gate, or when the divergence is accepted and stated, present tense, in the document that owns the file. Closing a finding deletes its section; the commit holds the evidence.
