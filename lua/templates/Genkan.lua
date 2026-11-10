----------------------------------------------------------------------------------------------------
-- Module Initialization
----------------------------------------------------------------------------------------------------

local _M = {}

----------------------------------------------------------------------------------------------------
-- Local Constants
----------------------------------------------------------------------------------------------------

DirectoryPagination = '/comics?page='

----------------------------------------------------------------------------------------------------
-- Event Functions
----------------------------------------------------------------------------------------------------

-- Get info and chapter list for current manga.
function _M.GetInfo()
  local n, v, x = nil
  local u = MaybeFillHost(MODULE.RootURL, URL)

  if not HTTP.GET(u) then return net_problem end

  x = CreateTXQuery(HTTP.Document)
  MANGAINFO.Title     = x.XPathString('//meta[@property="og:title"]/@content')
  MANGAINFO.CoverLink = x.XPathString('//meta[@property="og:image"]/@content')
  MANGAINFO.Summary   = x.XPathString('//meta[@property="og:description"]/@content')

  v = x.XPath('//a[contains(@class, "item-author")]')
  n = x.XPath('//span[contains(@class, "text-muted")]')
  for i = 1, v.Count do
    MANGAINFO.ChapterNames.Add('Chapter ' .. n.Get(i).ToString() .. ' - ' .. v.Get(i).ToString())
    MANGAINFO.ChapterLinks.Add(v.Get(i).GetAttribute('href'))
  end
  MANGAINFO.ChapterLinks.Reverse(); MANGAINFO.ChapterNames.Reverse()

  return no_error
end

-- Get the page count of the manga list of the current website.
function _M.GetDirectoryPageNumber()
  local u = MODULE.RootURL .. DirectoryPagination .. 1

  if not HTTP.GET(u) then return net_problem end

  PAGENUMBER = tonumber(CreateTXQuery(HTTP.Document).XPathString('//ul[@class="pagination"]/li[last()-1]/a')) or 1

  return no_error
end

-- Get LINKS and NAMES from the manga list of the current website.
function _M.GetNameAndLink()
  local x = nil
  local u = MODULE.RootURL .. DirectoryPagination .. (URL + 1)

  if not HTTP.GET(u) then return net_problem end

  x = CreateTXQuery(HTTP.Document)
  x.XPathHREFAll('//div[@id="content"]//a[contains(@class, "list-title")]', LINKS, NAMES)

  return no_error
end

-- Get the page count for the current chapter.
function _M.GetPageNumber()
  local v, x = nil
  local u = MaybeFillHost(MODULE.RootURL, URL)

  if not HTTP.GET(u) then return net_problem end

  x = CreateTXQuery(HTTP.Document)
  x.ParseHTML(GetBetween('window.chapterPages = ', ';', x.XPathString('//script[contains(., "window.chapterPages = ")]')):gsub('\\/', '/'))
  v = x.XPath('json(*)()')
  for i = 1, v.Count do
    TASK.PageLinks.Add(MaybeFillHost(MODULE.RootURL, v.Get(i).ToString()))
  end

  return no_error
end

----------------------------------------------------------------------------------------------------
-- Module After-Initialization
----------------------------------------------------------------------------------------------------

return _M
