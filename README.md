# qr-generator
A QR Code generator in Rust, written from scratch, ISO/IEC 18004-compliant. Includes a core encoder, CLI, and HTTP Service.

## Current Progress
- [x] Optimal Text Segmentation
- [ ] Data Encoding
- [ ] Error Correction Coding
- [ ] Structure Final Message
- [ ] Module Placement in Matrix
- [ ] Data Masking
- [ ] Format and Version Information
- [ ] QR Code CLI implementation
- [ ] QR Code SVG renderer
- [ ] QR Code PNG renderer
- [ ] QR Code API server

## References
I use the following references to research, implement and take inspiration from, with all due credit.
1. [Denso wave's QR Code product page](https://www.qrcode.com/en/)
2. [Thonky.com's QR Code tutorial](https://www.thonky.com/qr-code-tutorial/)
3. [Project Nayuki's Optimal text segmentation for QR codes](https://www.nayuki.io/page/optimal-text-segmentation-for-qr-codes)

## AI Use Disclosure
I have used the following "AI" tools in the specified capacity.
1. **Anthropic's Claude**:
    - Claude Code was used in [`optimal_segmentation.rs`](./qr-core/src/optimal_segmentation.rs) to correct the method `probe_ecc_level_raise`.
    - Claude Code was used in [`encoding.rs`](./qr-core/src/encoding.rs) to introduce the `Version` struct and remove redundant error handlings.
    - Claude chat was used to understand the Optimal Text Segmentation and develop the algorithm to implement.
    - Claude Code was used in [optimal_segmenation/tests.rs](./qr-core/src/optimal_segmentation/tests.rs) to write comprehensive test cases for all the text segmenation scenarios.
2. **Google's Gemini**:
    - Google's Gemini answers questions that you google about, so some sideline and Rust syntax research were done on Geimini in the way to normal googling.