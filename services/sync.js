import sha256 from 'crypto-js/sha256'
import Base64 from 'crypto-js/enc-base64'

const generateHash = (message, nonce) => {
  const hash = sha256(message + nonce)

  return hash.toString(Base64)
}

export default generateHash
