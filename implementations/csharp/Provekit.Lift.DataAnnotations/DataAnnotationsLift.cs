// SPDX-License-Identifier: Apache-2.0
//
// Provekit.Lift.DataAnnotations: lifts System.ComponentModel.DataAnnotations
// attributes to canonical ProvekIt IR.
//
// Cross-domain equivalence: the same validation constraint expressed via
// [Required], [Range], [StringLength], etc. produces byte-for-byte identical
// IR to Java Bean Validation @NotNull, @Min/@Max, @Size and Python
// pydantic Field(ge=, le=, min_length=, max_length=).
//
// Example:
//   public class User {
//       [Required, StringLength(100, MinimumLength = 1)]
//       public string Name { get; set; }
//
//       [Range(0, 150)]
//       public int Age { get; set; }
//
//       [EmailAddress]
//       public string Email { get; set; }
//   }
//
//   var decls = DataAnnotationsLift.LiftType<User>();
//   // -> 3 ContractDecls, one per property

using System.ComponentModel.DataAnnotations;
using System.Reflection;
using Provekit.IR;

namespace Provekit.Lift.DataAnnotations;

public static class DataAnnotationsLift
{
    /// <summary>
    /// Walk a type's public properties, read DataAnnotations validation
    /// attributes, and return one ContractDecl per property that has
    /// recognizable constraints.
    /// </summary>
    public static IReadOnlyList<ContractDecl> LiftType<T>() => LiftType(typeof(T));

    public static IReadOnlyList<ContractDecl> LiftType(Type type)
    {
        var decls = new List<ContractDecl>();
        var typeName = type.Name;

        foreach (var prop in type.GetProperties(BindingFlags.Public | BindingFlags.Instance))
        {
            var formulas = LiftProperty(prop);
            if (formulas is { Count: > 0 })
            {
                var pre = formulas.Count == 1 ? formulas[0] : Predicates.And([.. formulas]);
                decls.Add(new ContractDecl(
                    Name: $"{typeName}.{prop.Name}",
                    Pre: pre,
                    Post: null,
                    Inv: null,
                    OutBinding: "out"));
            }
        }
        return decls;
    }

    /// <summary>
    /// Extract IR formulas from a property's DataAnnotations attributes.
    /// Returns null if no liftable attributes are present.
    /// </summary>
    private static List<Formula>? LiftProperty(PropertyInfo prop)
    {
        var formulas = new List<Formula>();
        var varTerm = Terms.Var(prop.Name);
        var isString = prop.PropertyType == typeof(string);
        var isNumeric = IsNumericType(prop.PropertyType);

        foreach (var attr in prop.GetCustomAttributes<ValidationAttribute>())
        {
            var f = LiftAttribute(varTerm, prop.PropertyType, attr);
            if (f is not null)
                formulas.Add(f);
        }
        return formulas.Count > 0 ? formulas : null;
    }

    private static Formula? LiftAttribute(Term varTerm, Type propType, ValidationAttribute attr)
    {
        return attr switch
        {
            RequiredAttribute => RequiredToIr(varTerm, propType),
            RangeAttribute range => RangeToIr(varTerm, propType, range),
            StringLengthAttribute sl => StringLengthToIr(varTerm, sl),
            MinLengthAttribute min => MinLengthToIr(varTerm, min),
            MaxLengthAttribute max => MaxLengthToIr(varTerm, max),
            EmailAddressAttribute => Predicates.Atomic("kit:email", varTerm),
            UrlAttribute => Predicates.Atomic("kit:url", varTerm),
            PhoneAttribute => Predicates.Atomic("kit:phone", varTerm),
            CreditCardAttribute => Predicates.Atomic("kit:credit_card", varTerm),
            _ => null, // unrecognized: skip
        };
    }

    /// <summary>
    /// Vacuous-true validator attribute → kit-predicate name. Mirror of
    /// the Go validator lift's vacuousKitPredicate map; both adapters
    /// emit `Atomic("kit:&lt;tag&gt;", v)` for these so the IR position
    /// content-addresses via the OpacityManifest in
    /// protocol/specs/2026-05-02-opacity-manifest-grammar.md.
    /// </summary>
    public static readonly IReadOnlyList<string> VacuousKitPredicates = new[]
    {
        "kit:credit_card",
        "kit:email",
        "kit:phone",
        "kit:url",
    };

    // -----------------------------------------------------------------
    // Per-attribute IR mappings
    // -----------------------------------------------------------------

    /// <summary>
    /// [Required] → neq(var, "") for strings, neq(var, 0) for numerics.
    /// Mirrors Go validator "required" and Java @NotNull semantics.
    /// </summary>
    private static Formula RequiredToIr(Term v, Type propType)
    {
        if (propType == typeof(string))
            return Predicates.Ne(v, Terms.StrConst(""));
        if (IsNumericType(propType))
            return Predicates.Ne(v, Terms.Num(0));
        return Predicates.Ne(v, Terms.Num(0)); // fallback
    }

    /// <summary>
    /// [Range(min, max)] → and(gte(var, min), lte(var, max))
    /// Mirror of Java @Min(min) + @Max(max).
    /// </summary>
    private static Formula RangeToIr(Term v, Type propType, RangeAttribute range)
    {
        var constraints = new List<Formula>();

        if (range.Minimum is not null)
        {
            var min = Convert.ToInt64(range.Minimum);
            constraints.Add(Predicates.Gte(v, Terms.Num(min)));
        }
        if (range.Maximum is not null)
        {
            var max = Convert.ToInt64(range.Maximum);
            constraints.Add(Predicates.Lte(v, Terms.Num(max)));
        }

        return constraints.Count switch
        {
            0 => Predicates.And(),
            1 => constraints[0],
            _ => Predicates.And([.. constraints]),
        };
    }

    /// <summary>
    /// [StringLength(N)] → eq(strlen(var), N)
    /// [StringLength(N, MinimumLength = M)] → and(gte(strlen(var), M), lte(strlen(var), N))
    /// Mirror of Java @Size(min=M, max=N).
    /// </summary>
    private static Formula StringLengthToIr(Term v, StringLengthAttribute sl)
    {
        var strlen = Terms.Ctor("String.prototype.length", v);
        var constraints = new List<Formula>();

        if (sl.MinimumLength > 0)
            constraints.Add(Predicates.Gte(strlen, Terms.Num(sl.MinimumLength)));
        constraints.Add(Predicates.Lte(strlen, Terms.Num(sl.MaximumLength)));

        return constraints.Count == 1 ? constraints[0] : Predicates.And([.. constraints]);
    }

    /// <summary>
    /// [MinLength(N)] → gte(strlen(var), N)
    /// </summary>
    private static Formula MinLengthToIr(Term v, MinLengthAttribute min)
    {
        var strlen = Terms.Ctor("String.prototype.length", v);
        return Predicates.Gte(strlen, Terms.Num(min.Length));
    }

    /// <summary>
    /// [MaxLength(N)] → lte(strlen(var), N)
    /// </summary>
    private static Formula MaxLengthToIr(Term v, MaxLengthAttribute max)
    {
        var strlen = Terms.Ctor("String.prototype.length", v);
        return Predicates.Lte(strlen, Terms.Num(max.Length));
    }

    // -----------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------

    private static bool IsNumericType(Type t)
    {
        return t == typeof(int) || t == typeof(long) || t == typeof(short)
            || t == typeof(uint) || t == typeof(ulong) || t == typeof(ushort)
            || t == typeof(byte) || t == typeof(sbyte)
            || t == typeof(float) || t == typeof(double) || t == typeof(decimal);
    }
}
