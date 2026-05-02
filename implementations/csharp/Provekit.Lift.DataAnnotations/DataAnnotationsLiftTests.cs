// SPDX-License-Identifier: Apache-2.0

using System.ComponentModel.DataAnnotations;
using Provekit.IR;
using Provekit.Lift.DataAnnotations;
using Xunit;

namespace Provekit.Lift.DataAnnotations.Tests;

public class User
{
    [Required]
    [StringLength(100, MinimumLength = 1)]
    public string Name { get; set; } = "";

    [Range(0, 150)]
    public int Age { get; set; }

    [EmailAddress]
    public string Email { get; set; } = "";

    [MinLength(8)]
    public string Password { get; set; } = "";

    [MaxLength(500)]
    public string Bio { get; set; } = "";
}

public class Score
{
    [Range(0, 100)]
    public int Value { get; set; }
}

public class Plain
{
    public string Field { get; set; } = "";
}

public class Empty { }

public class DataAnnotationsLiftTests
{
    [Fact]
    public void LiftType_User_GetsAllProperties()
    {
        var decls = DataAnnotationsLift.LiftType<User>();
        Assert.Equal(5, decls.Count);

        var names = decls.Select(d => d.Name).ToHashSet();
        Assert.Contains("User.Name", names);
        Assert.Contains("User.Age", names);
        Assert.Contains("User.Email", names);
        Assert.Contains("User.Password", names);
        Assert.Contains("User.Bio", names);
    }

    [Fact]
    public void LiftType_RangeConstraint_ByteEquivalent()
    {
        // Same constraint as @Min(0) @Max(100) in Java Bean Validation
        // and pydantic Field(ge=0, le=100) in Python.
        var decls = DataAnnotationsLift.LiftType<Score>();
        Assert.Single(decls);

        var d = decls[0];
        Assert.NotNull(d.Pre);
        Assert.Equal("Score.Value", d.Name);

        var jcs = Serialize.MarshalDeclarations(new[] { d });
        Assert.Contains("\"kind\":\"and\"", jcs);
        Assert.Contains("\"kind\":\"atomic\",\"name\":\"≥\"", jcs);
        Assert.Contains("\"kind\":\"atomic\",\"name\":\"≤\"", jcs);
        Assert.Contains("\"kind\":\"var\",\"name\":\"Value\"", jcs);
        Assert.Contains("\"value\":0", jcs);
        Assert.Contains("\"value\":100", jcs);
    }

    [Fact]
    public void LiftType_RequiredString_NotNullEquivalent()
    {
        var decls = DataAnnotationsLift.LiftType<RequiredOnly>();
        Assert.Single(decls);
        Assert.Contains("≠", Serialize.MarshalDeclarations(decls));
    }

    [Fact]
    public void LiftType_StringLength_MapsToStrLen()
    {
        var decls = DataAnnotationsLift.LiftType<StringLengthOnly>();
        Assert.Single(decls);

        var jcs = Serialize.MarshalDeclarations(new[] { decls[0] });
        Assert.Contains("String.prototype.length", jcs);
        Assert.Contains("\"value\":50", jcs);
    }

    [Fact]
    public void LiftType_EmptyStruct_NoDeclarations()
    {
        var decls = DataAnnotationsLift.LiftType<Empty>();
        Assert.Empty(decls);
    }

    [Fact]
    public void LiftType_Plain_NoDeclarations()
    {
        var decls = DataAnnotationsLift.LiftType<Plain>();
        Assert.Empty(decls);
    }

    [Fact]
    public void LiftType_MinLength_ProducesGteStrLen()
    {
        var decls = DataAnnotationsLift.LiftType<MinLenOnly>();
        Assert.Single(decls);

        var jcs = Serialize.MarshalDeclarations(new[] { decls[0] });
        Assert.Contains("String.prototype.length", jcs);
        Assert.Contains("\"name\":\"≥\"", jcs);
        Assert.Contains("\"value\":8", jcs);
    }

    [Fact]
    public void LiftType_MaxLength_ProducesLteStrLen()
    {
        var decls = DataAnnotationsLift.LiftType<MaxLenOnly>();
        Assert.Single(decls);

        var jcs = Serialize.MarshalDeclarations(new[] { decls[0] });
        Assert.Contains("String.prototype.length", jcs);
        Assert.Contains("\"name\":\"≤\"", jcs);
        Assert.Contains("\"value\":500", jcs);
    }
}

// Test types for individual attribute checks
public class RequiredOnly { [Required] public string Name { get; set; } = ""; }
public class StringLengthOnly { [StringLength(50)] public string Title { get; set; } = ""; }
public class MinLenOnly { [MinLength(8)] public string Password { get; set; } = ""; }
public class MaxLenOnly { [MaxLength(500)] public string Bio { get; set; } = ""; }
