// Bridgeworks checked-add-u8 RTL native artifact.
// bridgeworks:claim rtl.alu.refines_add8
// bridgeworks:requires gates.full_adder.equations
// bridgeworks:projection rtl-lifter/v0
//
// Toy-lifter-visible contract:
//   For all 8-bit a,b, op ADD8 produces
//   sum == (a + b) mod 256 and carry == (a + b) >= 256.

module bridgeworks_checked_add_u8_alu (
    input  wire [7:0] a,
    input  wire [7:0] b,
    input  wire       op_add8,
    output wire [7:0] sum,
    output wire       carry
);
    wire [8:0] wide;

    assign wide = {1'b0, a} + {1'b0, b};
    assign sum = op_add8 ? wide[7:0] : 8'h00;
    assign carry = op_add8 ? wide[8] : 1'b0;
endmodule
