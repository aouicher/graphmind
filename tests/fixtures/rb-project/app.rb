require "json"
require_relative "helpers/auth"

module Authentication
  class User
    attr_reader :name, :email

    def initialize(name, email)
      @name = name
      @email = email
    end

    def self.find_by_email(email)
      # query database
    end

    def full_name
      format_name(name)
    end

    private

    def format_name(n)
      n.strip.capitalize
    end
  end
end

class ApiController
  def handle_request(request)
    user = Authentication::User.find_by_email(request[:email])
    validate_token(request[:token])
  end

  def validate_token(token)
    JWT.decode(token)
  end
end
