module RSpec
  module Lite
    @examples = []
    @failures = []

    class << self
      attr_reader :examples, :failures

      def describe(description, &block)
        group = ExampleGroup.new(description)
        group.instance_eval(&block)
      end

      def add(description, block)
        @examples << [description, block]
      end

      def run(files)
        files.each { |file| load file }
        @examples.each do |description, block|
          begin
            block.call
            print "."
          rescue => e
            @failures << [description, e]
            print "F"
          end
        end
        puts
        @failures.each_with_index do |(description, error), index|
          puts
          puts "#{index + 1}) #{description}"
          puts "   #{error.class}: #{error.message}"
          Array(error.backtrace).first(5).each { |line| puts "   #{line}" }
        end
        puts
        puts "#{@examples.length} examples, #{@failures.length} failures"
        @failures.empty? ? 0 : 1
      end
    end

    class ExampleGroup
      def initialize(description)
        @description = description
      end

      def describe(description, &block)
        self.class.new("#{@description} #{description}").instance_eval(&block)
      end

      def it(description, &block)
        RSpec::Lite.add("#{@description} #{description}", block)
      end

      def expect(actual)
        Expectation.new(actual)
      end
    end

    class Expectation
      def initialize(actual)
        @actual = actual
      end

      def to(matcher)
        matcher.matches?(@actual)
      end

      def not_to(matcher)
        matcher.does_not_match?(@actual)
      end
    end

    class Eq
      def initialize(expected)
        @expected = expected
      end

      def matches?(actual)
        raise "expected #{actual.inspect} to eq #{@expected.inspect}" unless actual == @expected
      end

      def does_not_match?(actual)
        raise "expected #{actual.inspect} not to eq #{@expected.inspect}" if actual == @expected
      end
    end

    class Include
      def initialize(expected)
        @expected = expected
      end

      def matches?(actual)
        raise "expected #{actual.inspect} to include #{@expected.inspect}" unless actual.include?(@expected)
      end

      def does_not_match?(actual)
        raise "expected #{actual.inspect} not to include #{@expected.inspect}" if actual.include?(@expected)
      end
    end

    class BeEmpty
      def matches?(actual)
        raise "expected #{actual.inspect} to be empty" unless actual.empty?
      end

      def does_not_match?(actual)
        raise "expected #{actual.inspect} not to be empty" if actual.empty?
      end
    end
  end
end

def describe(description, &block)
  RSpec::Lite.describe(description, &block)
end

def expect(actual)
  RSpec::Lite::Expectation.new(actual)
end

def eq(expected)
  RSpec::Lite::Eq.new(expected)
end

def include(expected)
  RSpec::Lite::Include.new(expected)
end

def be_empty
  RSpec::Lite::BeEmpty.new
end
