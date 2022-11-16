import type { NextApiRequest, NextApiResponse } from 'next'

interface Entry {
  id: string
  title: string
  url: string
}

interface Data {
  entries: Entry[]
}

const syncEntries = async (req: NextApiRequest, res: NextApiResponse<Data>) => {
  try {
    if (req.method === 'POST') {
      // TODO: save new entries record

      res.status(201).end()
    } else {
      // TODO: fetch latest entries record

      const entries = [{ id: '123', title: 'Hello', url: 'https://google.com' }]

      res.status(200).json({ entries })
    }
  } catch (err) {
    res.status(500).end()
    console.error('error running query', err)
  }
}

export default syncEntries
