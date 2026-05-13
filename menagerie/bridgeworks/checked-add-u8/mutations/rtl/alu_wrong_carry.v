// Bridgeworks red mutation: wrong carry bit.
// bridgeworks:claim rtl.alu.refines_add8
// bridgeworks:mutation expected_refusal=rtl_wrong_carry_bit

module bridgeworks_checked_add_u8_alu_mut_wrong_carry (
    input  wire [7:0] a,
    input  wire [7:0] b,
    output wire [7:0] sum,
    output wire       carry
);
    wire [8:0] wide;

    assign wide = {1'b0, a} + {1'b0, b};
    assign sum = wide[7:0];
    assign carry = wide[7]; // wrong: must be wide[8]
endmodule
